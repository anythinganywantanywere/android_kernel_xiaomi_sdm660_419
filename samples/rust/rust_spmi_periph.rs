// SPDX-License-Identifier: GPL-2.0

//! PMIC SPMI peripheral reader (Step 12).
//!
//! Reads two more PMIC peripherals straight over SPMI and
//! serves them on `/dev/rust_spmi_periph`:
//!
//! ```text
//! gpio0=0
//! gpio1=0
//! gpio2=0
//! gpio3=0
//! temp_stage=0
//! temp_thresh=1
//! ```
//!
//! The peripherals are the PMIC GPIO controller and the
//! temperature alarm, both SPMI slaves of the PMIC. Like the
//! RTC and ADC in Step 11, they already have C drivers
//! (`pinctrl-spmi-gpio`, `qcom-spmi-temp-alarm`), so no Rust
//! SPMI driver can bind to them; this driver is a platform
//! driver with a synthetic device and reaches them through a
//! direct SPMI handle (`SpmiDevice::for_slave(0)`, the PMIC
//! slave). Every read here is read-only.
//!
//! GPIO (`qcom,spmi-gpio`, base `0xc000`): each pin occupies
//! `0x100` bytes (`PMIC_GPIO_ADDRESS_RANGE`), and the
//! real-time status register is at offset `0x10` inside the
//! pin block (`PMIC_MPP_REG_RT_STS`); bit 0 is the pin level
//! (`PMIC_MPP_REG_RT_STS_VAL_MASK`). GPIO 0 is at `0xc010`,
//! GPIO 1 at `0xc110`, and so on.
//!
//! Temperature alarm (`qcom,temp-alarm@2400`, base `0x2400`):
//! the status register is at offset `0x08`
//! (`QPNP_TM_REG_STATUS`), and the shutdown-threshold control
//! at offset `0x40` (`QPNP_TM_REG_SHUTDOWN_CTRL1`, threshold
//! index in bits 1:0). The status stage is bits 1:0 on GEN1
//! (`STATUS_GEN1_STAGE_MASK`) and bits 6:4 on GEN2; the
//! reported stage is the raw bits 1:0, which is the stage on
//! this GEN1 part.

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
    type: RustSpmiPeriph,
    name: "rust_spmi_periph",
    author: "MayaKernel",
    description: "Rust PMIC SPMI peripheral reader (gpio + temp alarm)",
    license: "GPL",
}

/// PMIC GPIO controller base (`qcom,spmi-gpio`, `reg = <0xc000 ...>`).
const GPIO_BASE: u16 = 0xc000;
/// Bytes per GPIO pin (`PMIC_GPIO_ADDRESS_RANGE`).
const GPIO_STRIDE: u16 = 0x100;
/// Real-time status register offset inside a pin block.
const GPIO_RT_STS: u16 = 0x10;
/// Mask for the pin level in RT_STS.
const GPIO_RT_STS_VAL_MASK: u8 = 0x1;
/// Number of GPIO pins read (0..3).
const GPIO_COUNT: usize = 4;

/// Temperature alarm base (`qcom,temp-alarm@2400`).
const TEMP_ALARM_BASE: u16 = 0x2400;
/// Status register offset (`QPNP_TM_REG_STATUS`).
const TEMP_ALARM_STATUS: u16 = TEMP_ALARM_BASE + 0x08;
/// Shutdown-threshold control offset (`QPNP_TM_REG_SHUTDOWN_CTRL1`).
const TEMP_ALARM_SHUTDOWN_CTRL1: u16 = TEMP_ALARM_BASE + 0x40;
/// Threshold index mask (bits 1:0) in SHUTDOWN_CTRL1.
const TEMP_ALARM_THRESH_MASK: u8 = 0x3;
/// Stage mask (bits 1:0) in STATUS on GEN1.
const TEMP_ALARM_STAGE_MASK: u8 = 0x3;

/// Shared device state: the SPMI handle to the PMIC.
#[pin_data]
struct Shared {
    sdev: SpmiDevice,
}

impl Shared {
    /// Read one PMIC GPIO pin level (0 or 1). On a read error
    /// the pin reports 0.
    fn gpio(&self, pin: u16) -> u32 {
        let addr = GPIO_BASE + pin * GPIO_STRIDE + GPIO_RT_STS;
        match self.sdev.read_u8(addr) {
            Ok(v) => (v & GPIO_RT_STS_VAL_MASK) as u32,
            Err(_) => 0,
        }
    }

    /// Read the temp-alarm stage (bits 1:0 of STATUS).
    fn temp_stage(&self) -> u32 {
        match self.sdev.read_u8(TEMP_ALARM_STATUS) {
            Ok(v) => (v & TEMP_ALARM_STAGE_MASK) as u32,
            Err(_) => 0,
        }
    }

    /// Read the temp-alarm threshold index (bits 1:0 of
    /// SHUTDOWN_CTRL1).
    fn temp_thresh(&self) -> u32 {
        match self.sdev.read_u8(TEMP_ALARM_SHUTDOWN_CTRL1) {
            Ok(v) => (v & TEMP_ALARM_THRESH_MASK) as u32,
            Err(_) => 0,
        }
    }

    /// Render one snapshot: four GPIO levels, the temp-alarm
    /// stage and its threshold index.
    fn snapshot(&self) -> [u8; 128] {
        let mut buf = [0u8; 128];
        let mut len = 0usize;
        for pin in 0..GPIO_COUNT {
            len += copy_bytes(&mut buf[len..], b"gpio");
            len += write_u32_dec(&mut buf[len..], pin as u32);
            len += copy_bytes(&mut buf[len..], b"=");
            len += write_u32_dec(&mut buf[len..], self.gpio(pin as u16));
            len += copy_bytes(&mut buf[len..], b"\n");
        }
        len += copy_bytes(&mut buf[len..], b"temp_stage=");
        len += write_u32_dec(&mut buf[len..], self.temp_stage());
        len += copy_bytes(&mut buf[len..], b"\ntemp_thresh=");
        len += write_u32_dec(&mut buf[len..], self.temp_thresh());
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
struct SpmiPeriphOps {
    shared: Arc<Shared>,
}

impl FileOperations for SpmiPeriphOps {
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
struct RustSpmiPeriphDriver {
    misc: Option<MiscRegistration<SpmiPeriphOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustSpmiPeriphDriver {}
unsafe impl Sync for RustSpmiPeriphDriver {}

impl PlatformDriver for RustSpmiPeriphDriver {
    const NAME: &'static CStr = c_str!("rust_spmi_periph");

    fn probe(&mut self, _pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_spmi_periph: probe, opening SPMI handle to the PMIC\n");

        // A direct SPMI handle to the PMIC (slave 0). The GPIO
        // controller and the temp alarm already have C drivers,
        // so no Rust SPMI driver can bind to them; this handle
        // bypasses binding.
        let sdev = SpmiDevice::for_slave(0)?;
        // One read as the probe-time proof that the registers
        // are readable.
        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared { sdev }))?
            .into();
        let g0 = shared.gpio(0);
        let stage = shared.temp_stage();
        pr_info!(
            "rust_spmi_periph: gpio0 = {}, temp_stage = {}\n",
            g0, stage
        );

        let misc = MiscRegistration::<SpmiPeriphOps>::new(
            c_str!("rust_spmi_periph"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_spmi_periph: registered /dev/rust_spmi_periph\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_spmi_periph: remove, deregistering /dev/rust_spmi_periph\n");
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform driver registration plus the
/// synthetic platform device it binds to. Kept alive for the
/// module's lifetime; dropping it unregisters the driver (and
/// so runs `remove` on the bound device).
struct RustSpmiPeriph {
    _reg: platform::Registration<RustSpmiPeriphDriver>,
    _dev: platform::Device,
}

impl kernel::Module for RustSpmiPeriph {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_spmi_periph: registering platform driver\n");
        let drv = RustSpmiPeriphDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustSpmiPeriphDriver>::register(drv, module.as_ptr())?;
        // A synthetic platform device for the driver to bind
        // to (the GPIO controller and temp alarm are SPMI
        // devices, already owned by their C drivers).
        let dev = platform::Device::create(c_str!("rust_spmi_periph"), -1)?;
        Ok(Self { _reg: reg, _dev: dev })
    }
}
