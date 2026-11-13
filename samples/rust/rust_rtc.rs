// SPDX-License-Identifier: GPL-2.0

//! PMIC RTC reader (Step 11, option 3).
//!
//! Reads the hardware clock straight from the PMIC RTC
//! (`qcom,pm660_rtc`, `qcom,pm8941-rtc`) and serves it on
//! `/dev/rust_rtc`:
//!
//! ```text
//! rtc_secs=1763012345
//! ```
//!
//! The RTC is an SPMI slave of the PMIC, and its C driver
//! (`rtc-pm8xxx.c`) reads it through the PMIC regmap. A Rust
//! SPMI driver cannot bind to it, because the SPMI bus only
//! binds one driver per device and the C driver already owns
//! it (the first build proved this: the driver registered but
//! `probe` never ran). So this driver is a platform driver
//! with a synthetic device, and it reaches the RTC through a
//! direct SPMI handle (`SpmiDevice::for_slave(0)`, the PMIC
//! slave): the four time bytes at the read register
//! (`regs->read = 0x123`) are the seconds since the epoch,
//! little-endian. The read is read-only, so it cannot disturb
//! the C RTC driver; the value can be cross-checked against
//! `date +%s`.

use kernel::{
    bindings,
    c_str,
    file::{File, FileOperations, UserSliceWriter},
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    spmi::SpmiDevice,
    sync::{Arc, UniqueArc},
    text::{copy_bytes, write_u32_dec},
};

module! {
    type: RustRtc,
    name: "rust_rtc",
    author: "MayaKernel",
    description: "Rust PMIC RTC reader (spmi)",
    license: "GPL",
}

/// The RTC read register. This is `pm8941_regs.read` in
/// `rtc-pm8xxx.c`: the `qcom,pm8941-rtc` compatible (which the
/// PM660 uses) maps to `pm8941_regs`, whose read register is
/// `0x6048`. (The `0x123` value is `pm8921_regs.read`, for the
/// older PM8921 -- reading it on this PMIC returns 0.)
const RTC_READ_REG: u16 = 0x6048;
/// The time is four bytes, little-endian, seconds since epoch.
const RTC_TIME_LEN: usize = 4;

/// Shared device state: the SPMI handle to the PMIC.
#[pin_data]
struct Shared {
    sdev: SpmiDevice,
}

impl Shared {
    /// Read the RTC time (seconds since epoch) and render it.
    fn snapshot(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        let mut len = 0usize;
        let mut time_bytes = [0u8; RTC_TIME_LEN];
        let secs = match self.sdev.read_ext(RTC_READ_REG, &mut time_bytes) {
            Ok(()) => u32::from_le_bytes(time_bytes),
            Err(_) => 0,
        };
        len += copy_bytes(&mut buf[len..], b"rtc_secs=");
        len += write_u32_dec(&mut buf[len..], secs);
        len += copy_bytes(&mut buf[len..], b"\n");
        buf
    }

    /// The length of the snapshot text (up to the first NUL).
    fn snapshot_len(buf: &[u8; 32]) -> usize {
        buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
    }
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct RtcOps {
    shared: Arc<Shared>,
}

impl FileOperations for RtcOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // A live snapshot, not a stream: every read re-reads
        // the RTC. Honour the offset so `cat` sees EOF.
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
struct RustRtcDriver {
    misc: Option<MiscRegistration<RtcOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustRtcDriver {}
unsafe impl Sync for RustRtcDriver {}

impl PlatformDriver for RustRtcDriver {
    const NAME: &'static CStr = c_str!("rust_rtc");

    fn probe(&mut self, _pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_rtc: probe, opening SPMI handle to the PMIC\n");

        // A direct SPMI handle to the PMIC (slave 0). The RTC
        // itself already has a C driver, so no Rust SPMI driver
        // can bind to it; this handle bypasses binding.
        let sdev = SpmiDevice::for_slave(0)?;
        // Read the time once, as the probe-time proof that the
        // register is readable.
        let mut time_bytes = [0u8; RTC_TIME_LEN];
        sdev.read_ext(RTC_READ_REG, &mut time_bytes)?;
        let secs = u32::from_le_bytes(time_bytes);
        pr_info!("rust_rtc: time = {} secs since epoch\n", secs);

        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared { sdev }))?
            .into();
        let misc = MiscRegistration::<RtcOps>::new(
            c_str!("rust_rtc"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_rtc: registered /dev/rust_rtc\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_rtc: remove, deregistering /dev/rust_rtc\n");
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform driver registration plus the
/// synthetic platform device it binds to. Kept alive for the
/// module's lifetime; dropping it unregisters the driver (and
/// so runs `remove` on the bound device).
struct RustRtc {
    _reg: platform::Registration<RustRtcDriver>,
    _dev: platform::Device,
}

impl kernel::Module for RustRtc {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_rtc: registering platform driver\n");
        let drv = RustRtcDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustRtcDriver>::register(drv, module.as_ptr())?;
        // A synthetic platform device for the driver to bind
        // to (the RTC itself is an SPMI device, already owned
        // by its C driver).
        let dev = platform::Device::create(c_str!("rust_rtc"), -1)?;
        Ok(Self { _reg: reg, _dev: dev })
    }
}
