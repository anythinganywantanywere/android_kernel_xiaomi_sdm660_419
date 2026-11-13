// SPDX-License-Identifier: GPL-2.0

//! I2C probe driver (Step 9, option 2).
//!
//! The first consumer of the new `kernel::i2c` abstraction. It is
//! a platform driver (`rust_i2c`) with a synthetic device and a
//! misc device, `/dev/rust_i2c`, that reads one register live on
//! every `read`: the page register (0x00) of the Synaptics
//! touchscreen (`synaptics_dsx@20`, 7-bit address 0x20) on I2C bus
//! 1 (`i2c@c175000`, alias `i2c1`).
//!
//! The touchscreen is owned by the C `synaptics_dsx` driver, but
//! this only ever *reads* a single register, and `i2c_transfer`
//! serialises the transfer on the bus lock against the C driver's
//! own transfers, so it cannot corrupt the touchscreen. The read
//! returns one line, either the value or the error:
//!
//! ```text
//! synaptics_page=0x00
//! ```
//!
//! or, when the transfer fails:
//!
//! ```text
//! synaptics_page=error(-6)
//! ```

use kernel::{
    bindings,
    c_str,
    file::{File, FileOperations, UserSliceWriter},
    i2c::I2cAdapter,
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    sync::{Arc, UniqueArc},
};
use kernel::text::{copy_bytes, write_u32_dec, write_i32_dec};

module! {
    type: RustI2c,
    name: "rust_i2c",
    author: "MayaKernel",
    description: "Rust I2C probe (Synaptics page register)",
    license: "GPL",
}

/// I2C bus number of the touchscreen (`i2c@c175000`, alias `i2c1`).
const SYNAPTICS_BUS: i32 = 1;
/// 7-bit I2C address of the Synaptics touchscreen.
const SYNAPTICS_ADDR: u16 = 0x20;
/// The page register: safe to read, returns the current page.
const SYNAPTICS_PAGE_REG: u8 = 0x00;

/// Shared device state: the adapter and the device address. Held
/// behind an `Arc` so the file operations can read the register.
#[pin_data]
struct Shared {
    adapter: I2cAdapter,
    addr: u16,
}

impl Shared {
    /// Read the page register live and render one text line.
    fn snapshot(&self) -> [u8; 64] {
        let mut buf = [0u8; 64];
        let mut len = 0usize;
        len += copy_bytes(&mut buf[len..], b"synaptics_page=");
        match self.adapter.read_reg(self.addr, SYNAPTICS_PAGE_REG) {
            Ok(v) => {
                len += copy_bytes(&mut buf[len..], b"0x");
                len += write_u8_hex(&mut buf[len..], v);
            }
            Err(e) => {
                len += copy_bytes(&mut buf[len..], b"error(");
                len += write_i32_dec(&mut buf[len..], e.to_errno());
                len += copy_bytes(&mut buf[len..], b")");
            }
        }
        len += copy_bytes(&mut buf[len..], b"\n");
        buf
    }

    /// The length of the snapshot text (up to the first NUL).
    fn snapshot_len(buf: &[u8; 64]) -> usize {
        buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
    }
}

/// Write a `u8` as two lowercase hex digits into `out`.
fn write_u8_hex(out: &mut [u8], value: u8) -> usize {
    let hi = (value >> 4) & 0xf;
    let lo = value & 0xf;
    let mut n = 0usize;
    if n < out.len() {
        out[n] = if hi < 10 { b'0' + hi } else { b'a' + (hi - 10) };
        n += 1;
    }
    if n < out.len() {
        out[n] = if lo < 10 { b'0' + lo } else { b'a' + (lo - 10) };
        n += 1;
    }
    n
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct I2cOps {
    shared: Arc<Shared>,
}

impl FileOperations for I2cOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // A live snapshot, not a stream: every read re-reads
        // the register. Honour the offset so `cat` sees EOF.
        let buf = self.shared.snapshot();
        let len = Shared::snapshot_len(&buf);
        let offset = offset as usize;
        if offset >= len {
            return Ok(0);
        }
        writer.write(&buf[offset..])
    }
}

/// Driver state. The misc device is registered in `probe`, so
/// `misc` and `shared` start empty; the module pointer is the
/// only field `init` fills in.
struct RustI2cDriver {
    misc: Option<MiscRegistration<I2cOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustI2cDriver {}
unsafe impl Sync for RustI2cDriver {}

impl PlatformDriver for RustI2cDriver {
    const NAME: &'static CStr = c_str!("rust_i2c");

    fn probe(&mut self, pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_i2c: probe, driver bound\n");

        // Open I2C bus 1 and read the Synaptics page register
        // once, as the probe-time proof that the transfer works.
        let adapter = I2cAdapter::get(SYNAPTICS_BUS)?;
        match adapter.read_reg(SYNAPTICS_ADDR, SYNAPTICS_PAGE_REG) {
            Ok(v) => pr_info!("rust_i2c: synaptics page = {:#x}\n", v),
            Err(e) => pr_info!("rust_i2c: synaptics read failed: {}\n", e.to_errno()),
        }

        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared {
                adapter,
                addr: SYNAPTICS_ADDR,
            }))?
            .into();
        let misc = MiscRegistration::<I2cOps>::new(
            c_str!("rust_i2c"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_i2c: registered /dev/rust_i2c\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_i2c: remove, deregistering /dev/rust_i2c\n");
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform device and the driver registration.
/// Kept alive for the module's lifetime; dropping them unregisters
/// the device (and so runs `remove`) and the driver.
struct RustI2c {
    _dev: platform::Device,
    _reg: platform::Registration<RustI2cDriver>,
}

impl kernel::Module for RustI2c {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_i2c: registering platform driver\n");
        let drv = RustI2cDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustI2cDriver>::register(drv, module.as_ptr())?;
        pr_info!("rust_i2c: creating platform device\n");
        let dev = platform::Device::create(c_str!("rust_i2c"), -1)?;
        Ok(Self { _dev: dev, _reg: reg })
    }
}
