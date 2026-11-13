// SPDX-License-Identifier: GPL-2.0

//! PWM LPG reader (Step 13).
//!
//! Reads the display-backlight PWM duty and enable state
//! straight from the PMIC LPG block and serves them on
//! `/dev/rust_pwm`:
//!
//! ```text
//! pwm2_value=256
//! pwm2_enabled=1
//! pwm1_value=0
//! pwm1_enabled=0
//! ```
//!
//! The LPG block is `qcom,pwms@b100` (`qcom,pwm-lpg`,
//! `lpg-base = 0xb100`, 3 channels), on PM660L
//! (`qcom,pm660l@3`, SPMI slave 3). The backlight uses
//! channel 2 (`pwms = <0x1b 0x02 0xf4240>`); channel 1 is
//! the other LPG channel on the same block. The duty is a
//! 9-bit value (0-511). Every read here is read-only, so
//! the brightness cannot change. The LPG already has a C
//! driver, so this driver is a platform driver with a
//! synthetic device and reads through a direct SPMI handle
//! to PM660L (`SpmiDevice::for_slave(3)`), like the RTC
//! and battery drivers in Step 11.

use kernel::{
    bindings,
    c_str,
    file::{File, FileOperations, UserSliceWriter},
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    pwm::PwmLpg,
    spmi::SpmiDevice,
    sync::{Arc, UniqueArc},
    text::{copy_bytes, write_u32_dec},
};

module! {
    type: RustPwm,
    name: "rust_pwm",
    author: "MayaKernel",
    description: "Rust PWM LPG reader (backlight duty)",
    license: "GPL",
}

/// LPG block base (`qcom,pwms@b100`, `lpg-base`).
const LPG_BASE: u16 = 0xb100;
/// Backlight PWM channel (from the backlight node's `pwms`).
const BACKLIGHT_CHAN: u16 = 2;
/// The other LPG channel on the same block.
const OTHER_CHAN: u16 = 1;

/// Shared device state: the SPMI handle to the PMIC.
#[pin_data]
struct Shared {
    sdev: SpmiDevice,
}

impl Shared {
    /// Render one snapshot: duty value and enable state for
    /// the backlight channel and the other channel. On a read
    /// error the value reports 0 and enabled reports 0.
    fn snapshot(&self) -> [u8; 128] {
        let mut buf = [0u8; 128];
        let mut len = 0usize;
        for (label, chan) in [("pwm2", BACKLIGHT_CHAN), ("pwm1", OTHER_CHAN)] {
            let lpg = PwmLpg::new(&self.sdev, LPG_BASE, chan);
            let value = lpg.value().unwrap_or(0);
            let enabled: u32 = if lpg.is_enabled().unwrap_or(false) { 1 } else { 0 };
            len += copy_bytes(&mut buf[len..], label.as_bytes());
            len += copy_bytes(&mut buf[len..], b"_value=");
            len += write_u32_dec(&mut buf[len..], value as u32);
            len += copy_bytes(&mut buf[len..], b"\n");
            len += copy_bytes(&mut buf[len..], label.as_bytes());
            len += copy_bytes(&mut buf[len..], b"_enabled=");
            len += write_u32_dec(&mut buf[len..], enabled);
            len += copy_bytes(&mut buf[len..], b"\n");
        }
        buf
    }

    /// The length of the snapshot text (up to the first NUL).
    fn snapshot_len(buf: &[u8; 128]) -> usize {
        buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
    }
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct PwmOps {
    shared: Arc<Shared>,
}

impl FileOperations for PwmOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // A live snapshot, not a stream: every read re-reads
        // the hardware. Honour the offset so `cat` sees EOF.
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
struct RustPwmDriver {
    misc: Option<MiscRegistration<PwmOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustPwmDriver {}
unsafe impl Sync for RustPwmDriver {}

impl PlatformDriver for RustPwmDriver {
    const NAME: &'static CStr = c_str!("rust_pwm");

    fn probe(&mut self, _pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_pwm: probe, opening SPMI handle to the PMIC\n");

        // A direct SPMI handle to the PMIC (slave 3). The LPG
        // lives on PM660L (`qcom,pm660l@3`, `reg = <0x03 ...>`),
        // not on the main PMIC, so slave 0 reads zero. The LPG
        // already has a C driver, so no Rust SPMI driver can
        // bind to it; this handle bypasses binding.
        let sdev = SpmiDevice::for_slave(3)?;
        // One read as the probe-time proof that the registers
        // are readable.
        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared { sdev }))?
            .into();
        let lpg = PwmLpg::new(&shared.sdev, LPG_BASE, BACKLIGHT_CHAN);
        let value = lpg.value().unwrap_or(0);
        let enabled = lpg.is_enabled().unwrap_or(false);
        pr_info!(
            "rust_pwm: backlight pwm2 value = {}, enabled = {}\n",
            value, enabled as u32
        );

        let misc = MiscRegistration::<PwmOps>::new(
            c_str!("rust_pwm"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_pwm: registered /dev/rust_pwm\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_pwm: remove, deregistering /dev/rust_pwm\n");
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform driver registration plus the
/// synthetic platform device it binds to. Kept alive for the
/// module's lifetime; dropping it unregisters the driver (and
/// so runs `remove` on the bound device).
struct RustPwm {
    _reg: platform::Registration<RustPwmDriver>,
    _dev: platform::Device,
}

impl kernel::Module for RustPwm {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_pwm: registering platform driver\n");
        let drv = RustPwmDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustPwmDriver>::register(drv, module.as_ptr())?;
        // A synthetic platform device for the driver to bind
        // to (the LPG is an SPMI device, already owned by its
        // C driver).
        let dev = platform::Device::create(c_str!("rust_pwm"), -1)?;
        Ok(Self { _reg: reg, _dev: dev })
    }
}
