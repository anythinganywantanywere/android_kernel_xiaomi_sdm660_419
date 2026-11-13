// SPDX-License-Identifier: GPL-2.0

//! SMB1351 charger status reader (Step 14, part 1).
//!
//! Reads the parallel charger's status straight over I2C and
//! serves it on `/dev/rust_charger`:
//!
//! ```text
//! chg_status4=0x02
//! chg_hold_off=0
//! chg_charging=1
//! chg_done=0
//! ```
//!
//! The charger is `smb1351-charger@1d` on I2C bus 2
//! (`qcom,smb1351-charger`, `qcom,parallel-charger`). The
//! status lives in `STATUS_4_REG` (`0x3A`): bit 3 is hold-off
//! (`STATUS_HOLD_OFF_BIT`), bits 2:1 are the charge state
//! (`STATUS_CHG_MASK`), bit 5 is charge-done
//! (`STATUS_DONE_BIT`). The decode follows the C driver's
//! `smb1351_get_prop_batt_status`: hold-off means not
//! charging, any charge bit means charging, otherwise
//! discharging. Every read here is read-only, so charging
//! cannot be disturbed; the bus lock serializes this against
//! the C charger driver.

use kernel::{
    bindings,
    c_str,
    file::{File, FileOperations, UserSliceWriter},
    i2c::I2cAdapter,
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    sync::{Arc, UniqueArc},
    text::{copy_bytes, write_u32_dec, write_u32_hex},
};

module! {
    type: RustCharger,
    name: "rust_charger",
    author: "MayaKernel",
    description: "Rust SMB1351 charger status reader (i2c)",
    license: "GPL",
}

/// I2C bus carrying the SMB1351 (`smb1351-charger@1d`).
const SMB1351_BUS: i32 = 2;
/// SMB1351 I2C address.
const SMB1351_ADDR: u16 = 0x1d;
/// Charge-status register (`STATUS_4_REG`).
const SMB1351_STATUS_4: u8 = 0x3A;
/// Hold-off bit in STATUS_4 (`STATUS_HOLD_OFF_BIT`).
const SMB1351_HOLD_OFF_BIT: u8 = 0x08;
/// Charge-state bits in STATUS_4 (`STATUS_CHG_MASK`, bits 2:1).
const SMB1351_CHG_MASK: u8 = 0x06;
/// Charge-done bit in STATUS_4 (`STATUS_DONE_BIT`).
const SMB1351_DONE_BIT: u8 = 0x20;

/// Shared device state: the I2C adapter for bus 2.
#[pin_data]
struct Shared {
    adapter: I2cAdapter,
}

impl Shared {
    /// Read STATUS_4 from the charger. Returns 0 on error.
    fn status4(&self) -> u8 {
        self.adapter
            .read_reg(SMB1351_ADDR, SMB1351_STATUS_4)
            .unwrap_or(0)
    }

    /// Render one snapshot: the raw STATUS_4 byte plus the
    /// decoded hold-off / charging / done flags.
    fn snapshot(&self) -> [u8; 128] {
        let mut buf = [0u8; 128];
        let mut len = 0usize;
        let st = self.status4();
        let hold_off: u32 = if st & SMB1351_HOLD_OFF_BIT != 0 { 1 } else { 0 };
        let charging: u32 = if st & SMB1351_CHG_MASK != 0 { 1 } else { 0 };
        let done: u32 = if st & SMB1351_DONE_BIT != 0 { 1 } else { 0 };
        len += copy_bytes(&mut buf[len..], b"chg_status4=0x");
        len += write_u32_hex(&mut buf[len..], st as u32);
        len += copy_bytes(&mut buf[len..], b"\nchg_hold_off=");
        len += write_u32_dec(&mut buf[len..], hold_off);
        len += copy_bytes(&mut buf[len..], b"\nchg_charging=");
        len += write_u32_dec(&mut buf[len..], charging);
        len += copy_bytes(&mut buf[len..], b"\nchg_done=");
        len += write_u32_dec(&mut buf[len..], done);
        len += copy_bytes(&mut buf[len..], b"\n");
        buf
    }

    /// The length of the snapshot text (up to the first NUL).
    fn snapshot_len(buf: &[u8; 128]) -> usize {
        buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
    }
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct ChargerOps {
    shared: Arc<Shared>,
}

impl FileOperations for ChargerOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // A live snapshot, not a stream: every read re-reads
        // the charger. Honour the offset so `cat` sees EOF.
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
struct RustChargerDriver {
    misc: Option<MiscRegistration<ChargerOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustChargerDriver {}
unsafe impl Sync for RustChargerDriver {}

impl PlatformDriver for RustChargerDriver {
    const NAME: &'static CStr = c_str!("rust_charger");

    fn probe(&mut self, _pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_charger: probe, opening I2C bus {}\n", SMB1351_BUS);

        let adapter = I2cAdapter::get(SMB1351_BUS)?;
        // One read as the probe-time proof that the charger
        // answers.
        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared { adapter }))?
            .into();
        let st = shared.status4();
        pr_info!("rust_charger: STATUS_4 = {:#x}\n", st);

        let misc = MiscRegistration::<ChargerOps>::new(
            c_str!("rust_charger"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_charger: registered /dev/rust_charger\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_charger: remove, deregistering /dev/rust_charger\n");
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform driver registration plus the
/// synthetic platform device it binds to. Kept alive for the
/// module's lifetime; dropping it unregisters the driver (and
/// so runs `remove` on the bound device).
struct RustCharger {
    _reg: platform::Registration<RustChargerDriver>,
    _dev: platform::Device,
}

impl kernel::Module for RustCharger {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_charger: registering platform driver\n");
        let drv = RustChargerDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustChargerDriver>::register(drv, module.as_ptr())?;
        // A synthetic platform device for the driver to bind
        // to (the charger is an I2C device, already owned by
        // its C driver).
        let dev = platform::Device::create(c_str!("rust_charger"), -1)?;
        Ok(Self { _reg: reg, _dev: dev })
    }
}
