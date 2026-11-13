// SPDX-License-Identifier: GPL-2.0

//! Hardware monitor driver: the Step 8 pivot.
//!
//! The original Step 8 (the QRNG port, `rust_rng`) cannot run on
//! this device: its registers fault with an asynchronous SError
//! even with the interface clock enabled and the NoC bus voted,
//! and the C `msm_rng` driver does not run here either (no
//! `/dev/hwrng`). So Step 8 pivots to hardware that is proven
//! accessible on this device: everything the Steps 2-7 proof
//! driver already verified on-device (GIC TYPER, the xo clock,
//! a GPIO input, and a regulator), exposed as a single read-only
//! misc device.
//!
//! The driver is a platform driver (`rust_hwmon`) with a
//! synthetic device. On `read`, it re-reads the hardware live
//! and returns a text snapshot, one `key=value` line per
//! resource:
//!
//! ```text
//! gic_typer=0x7b04f3
//! xo_rate_hz=19200000
//! gpio64_camera_focus=0
//! pm660_l1_uv=1200000
//! pm660_l1_enabled=1
//! ```
//!
//! No bus vote is needed: every resource here was already read
//! in Steps 2-7 without one. The QRNG registers are never
//! touched.

use core::sync::atomic::{AtomicBool, Ordering};

use kernel::{
    bindings,
    c_str,
    clk::Clk,
    file::{File, FileOperations, UserSliceReader, UserSliceWriter},
    gpio::Gpio,
    io::IoMem,
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    regulator::Regulator,
    sync::{Arc, UniqueArc},
};
use kernel::text::{copy_bytes, write_u32_dec, write_u32_hex, write_line};

module! {
    type: RustHwmon,
    name: "rust_hwmon",
    author: "MayaKernel",
    description: "Rust hardware monitor (proven-resources snapshot)",
    license: "GPL",
}

/// GIC distributor base and size (`sdm660.dtsi`).
const GICD_BASE: u64 = 0x17a0_0000;
const GICD_SIZE: usize = 0x10000;
/// GICD_TYPER offset.
const GICD_TYPER: usize = 0x004;

/// Fixed xo clock node name.
const XO_NODE: &CStr = c_str!("xo_board");
/// Camera-focus GPIO (a read-only input, owned by `gpio-keys`).
const GPIO_KEY_FOCUS_NODE: &CStr = c_str!("camera_focus");
const GPIO_KEY_FOCUS_PROP: &CStr = c_str!("gpios");
/// TLMM hardware offsets of the focus key and the test output pin.
const GPIO_KEY_FOCUS: u32 = 64;
const GPIO_TEST_OUT: u32 = 100;
/// pm660 L1 regulator name.
const PM660_L1: &CStr = c_str!("pm660_l1");

/// Shared device state: the mapped GIC registers, the xo clock,
/// the focus GPIO number, and the L1 regulator. Held behind an
/// `Arc` so the file operations can read the hardware; created in
/// `probe` and dropped when the last user (driver or open file)
/// lets go.
#[pin_data]
struct Shared {
    iomem: IoMem,
    clk: Clk,
    gpio_key: u32,
    reg: Regulator,
    /// Unused TLMM output pin (Step 7 proof), driven by `write`.
    gpio_out: Gpio,
    /// What `write` last drove the pin to. The physical
    /// read-back (`gpiod_get_value`, the pin's input bit) is
    /// unreliable for this unused pin: its function can be
    /// reset after boot, so the input bit reads the floating
    /// pin (0) even while the output driver is told to drive
    /// high. Track the driven value instead.
    out_state: AtomicBool,
}

impl Shared {
    /// Read the hardware live and render one text snapshot.
    /// The buffer is a fixed stack array; the snapshot is short
    /// (six lines) and always fits.
    fn snapshot(&self) -> [u8; 160] {
        let mut buf = [0u8; 160];
        let mut len = 0usize;
        // GIC TYPER (read-only register), shown in hex.
        let typer = self.iomem.readl(GICD_TYPER);
        len += copy_bytes(&mut buf[len..], b"gic_typer=0x");
        len += write_u32_hex(&mut buf[len..], typer);
        len += copy_bytes(&mut buf[len..], b"\n");
        // xo clock rate.
        len += write_line(&mut buf[len..], b"xo_rate_hz=", self.clk.rate() as u32);
        // Camera-focus GPIO (input, read-only use).
        let key = Gpio::from_number(self.gpio_key)
            .and_then(|g| g.get_value())
            .unwrap_or(false);
        len += write_line(&mut buf[len..], b"gpio64_camera_focus=", key as u32);
        // pm660 L1 regulator voltage and state.
        let uv = self.reg.voltage().unwrap_or(0);
        len += write_line(&mut buf[len..], b"pm660_l1_uv=", uv as u32);
        let enabled = self.reg.is_enabled().unwrap_or(false);
        len += write_line(&mut buf[len..], b"pm660_l1_enabled=", enabled as u32);
        // Test output pin state: what `write` last drove it to
        // (see the `out_state` comment in `Shared`).
        let out = self.out_state.load(Ordering::Relaxed);
        len += write_line(&mut buf[len..], b"gpio100_out=", out as u32);
        buf
    }

    /// The length of the snapshot text (up to the first NUL).
    fn snapshot_len(buf: &[u8; 160]) -> usize {
        buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
    }
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct HwmonOps {
    shared: Arc<Shared>,
}

impl FileOperations for HwmonOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // A snapshot, not a stream: every read renders the
        // current hardware state live, so a second open sees a
        // changed GPIO or regulator state. But the `offset`
        // MUST be honoured: once the reader has consumed the
        // whole snapshot, return 0 (EOF). Ignoring the offset
        // makes `cat` loop forever, re-printing the same five
        // lines until it is killed.
        let buf = self.shared.snapshot();
        let len = Shared::snapshot_len(&buf);
        let offset = offset as usize;
        if offset >= len {
            return Ok(0);
        }
        writer.write(&buf[offset..])
    }

    fn write(&self, _file: &File, reader: &mut UserSliceReader, _offset: u64) -> Result<usize> {
        // Drive the test output pin (TLMM GPIO 100, unused by
        // anything else). The first byte decides: `1` drives it
        // high, `0` drives it low; anything else is ignored but
        // still consumed. `echo 1 > /dev/rust_hwmon` then
        // `cat /dev/rust_hwmon` shows `gpio100_out=1`.
        let mut byte = [0u8; 1];
        let n = reader.read(&mut byte)?;
        if n == 0 {
            return Ok(0);
        }
        match byte[0] {
            b'1' => {
                // Re-assert the direction as well as the value:
                // `set_value` alone leaves the pin as-is if
                // something reset it to an input since `probe`,
                // and then the read-back in `snapshot` shows the
                // floating input (0) instead of the driven value.
                // `direction_output` sets both at once.
                if let Err(e) = self.shared.gpio_out.direction_output(true) {
                    pr_info!("rust_hwmon: gpio100_out direction failed: {}\n", e.to_errno());
                    return Err(e);
                }
                self.shared.out_state.store(true, Ordering::Relaxed);
                pr_info!("rust_hwmon: gpio100_out = 1\n");
            }
            b'0' => {
                if let Err(e) = self.shared.gpio_out.direction_output(false) {
                    pr_info!("rust_hwmon: gpio100_out direction failed: {}\n", e.to_errno());
                    return Err(e);
                }
                self.shared.out_state.store(false, Ordering::Relaxed);
                pr_info!("rust_hwmon: gpio100_out = 0\n");
            }
            _ => {}
        }
        Ok(n)
    }
}

/// Driver state. The misc device is registered in `probe` (not
/// `init`), so `misc` and `shared` start empty; the module
/// pointer is the only field `init` fills in.
struct RustHwmonDriver {
    misc: Option<MiscRegistration<HwmonOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustHwmonDriver {}
unsafe impl Sync for RustHwmonDriver {}

impl PlatformDriver for RustHwmonDriver {
    const NAME: &'static CStr = c_str!("rust_hwmon");

    fn probe(&mut self, pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_hwmon: probe, driver bound\n");

        // GIC distributor (proven readable in Step 3).
        let iomem = IoMem::map(GICD_BASE, GICD_SIZE)?;
        let typer = iomem.readl(GICD_TYPER);
        pr_info!("rust_hwmon: GIC TYPER = {:#x}\n", typer);

        // xo clock (proven in Step 4): provider form, no
        // `clocks` property on the node itself.
        let clk = Clk::of_get(XO_NODE)?;
        clk.enable()?;
        pr_info!("rust_hwmon: xo rate = {} Hz\n", clk.rate());

        // Camera-focus GPIO number (proven in Steps 4-7: the
        // TLMM base is resolved from this node's `gpios`).
        let gpio_key = kernel::gpio::number_from_of(GPIO_KEY_FOCUS_NODE, GPIO_KEY_FOCUS_PROP, 0)?;
        let key = Gpio::from_number(gpio_key)?;
        let key_val = key.get_value()?;
        pr_info!("rust_hwmon: gpio 64 (camera focus, linux {}) = {}\n", gpio_key, key_val);

        // pm660 L1 regulator (proven in Step 7).
        let reg = Regulator::get(PM660_L1)?;
        let uv = reg.voltage()?;
        let enabled = reg.is_enabled()?;
        pr_info!("rust_hwmon: pm660_l1 = {} uV, enabled = {}\n", uv, enabled);

        // Test output pin (proven in Step 7): the TLMM base
        // follows from the focus key's Linux number, so the
        // unreferenced pin is the same base plus its offset.
        let out_linux = gpio_key + (GPIO_TEST_OUT - GPIO_KEY_FOCUS);
        let gpio_out = Gpio::from_number(out_linux)?;
        gpio_out.direction_output(false)?;
        pr_info!("rust_hwmon: gpio {} out (linux {}) = 0\n", GPIO_TEST_OUT, out_linux);

        // Build the shared state, then register the misc device.
        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared {
                iomem,
                clk,
                gpio_key,
                reg,
                gpio_out,
                out_state: AtomicBool::new(false),
            }))?
            .into();
        let misc = MiscRegistration::<HwmonOps>::new(
            c_str!("rust_hwmon"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_hwmon: registered /dev/rust_hwmon\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_hwmon: remove, deregistering /dev/rust_hwmon\n");
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform device and the driver registration.
/// Kept alive for the module's lifetime; dropping them unregisters
/// the device (and so runs `remove`) and the driver.
struct RustHwmon {
    _dev: platform::Device,
    _reg: platform::Registration<RustHwmonDriver>,
}

impl kernel::Module for RustHwmon {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_hwmon: registering platform driver\n");
        let drv = RustHwmonDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustHwmonDriver>::register(drv, module.as_ptr())?;
        pr_info!("rust_hwmon: creating platform device\n");
        let dev = platform::Device::create(c_str!("rust_hwmon"), -1)?;
        Ok(Self { _dev: dev, _reg: reg })
    }
}
