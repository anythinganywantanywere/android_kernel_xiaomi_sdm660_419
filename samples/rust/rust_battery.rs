// SPDX-License-Identifier: GPL-2.0

//! PMIC ADC battery-voltage reader (Step 11, option 1).
//!
//! Reads the battery voltage straight from the PMIC ADC
//! (`vadc@3100`, `qcom,spmi-adc-rev2`) and serves it on
//! `/dev/rust_battery`:
//!
//! ```text
//! vbatt_uv=4123456
//! vbatt_mv=4123
//! raw_code=0x1a2b
//! ```
//!
//! The ADC is an SPMI slave of the PMIC, and its C driver
//! (`qcom-spmi-adc5.c`) talks to it through the PMIC regmap. A
//! Rust SPMI driver cannot bind to it, because the SPMI bus
//! only binds one driver per device and the C driver already
//! owns it (the first build proved this: the driver registered
//! but `probe` never ran). So this driver is a platform driver
//! with a synthetic device, and it reaches the ADC through a
//! direct SPMI handle (`SpmiDevice::for_slave(0)`, the PMIC
//! slave). The conversion follows the C driver's sequence --
//! select the `VBAT_SNS` channel, enable the ADC, request a
//! conversion, poll for end-of-conversion, read the result --
//! and its scaling. The value can be cross-checked against
//! `/sys/class/power_supply/battery/voltage_now`.
//!
//! Registers are offsets from the ADC base `0x3100`
//! (`vadc@3100`, `reg = <0x3100 0x100>`). The battery voltage
//! is read from the `VPH_PWR` channel (`reg = <0x83>` in the
//! device tree, pre-scaling 1/3): VPH is the main power rail,
//! which is the battery voltage when the phone runs on
//! battery. (There is no `vbat_sns` channel in this device's
//! ADC device tree, and the `3` in the C driver's
//! `ADC_CHAN_VOLT("vbat_sns", 3, ...)` is a prescale index,
//! not a channel number.) The scale is the C driver's
//! `qcom_vadc_scale_hw_calib_volt` for `SCALE_HW_CALIB_DEFAULT`:
//! `uV = raw * 1875mV * 1000 / 0x4000 * 3`, where `0x4000`
//! (`full_scale_code_volt`) is the `data_pmic_rev2` full scale
//! for this ADC revision and `1875mV` is the ADC reference.
//! (`VADC5_MAX_CODE` is `0x7fff`; the full scale for voltage is
//! `0x4000`, not the max code.)
//!
//! The ADC is shared with the C driver, which converts channels
//! for battery monitoring on its own schedule. This driver does
//! one conversion per read and holds no ADC state, so a
//! conversion here is a brief, self-contained transaction; the
//! C driver re-reads on its next poll. The channel select and
//! the enable are read-modify-write, so the C driver's other
//! ADC configuration (decimation, averaging, settle time) is
//! left untouched.

use kernel::{
    bindings,
    c_str,
    file::{File, FileOperations, UserSliceWriter},
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    spmi::SpmiDevice,
    sync::{Arc, UniqueArc},
    text::{copy_bytes, write_u32_dec, write_u32_hex},
};

module! {
    type: RustBattery,
    name: "rust_battery",
    author: "MayaKernel",
    description: "Rust PMIC ADC battery reader (spmi)",
    license: "GPL",
}

/// ADC register base (`vadc@3100`).
const ADC_BASE: u16 = 0x3100;
/// Status register 1; bit 0 is end-of-conversion.
const ADC_USR_STATUS1: u16 = ADC_BASE + 0x8;
/// Mask for the request-status / EOC bits.
const ADC_USR_STATUS1_REQ_STS_EOC_MASK: u8 = 0x3;
/// EOC bit: the conversion is done.
const ADC_USR_STATUS1_EOC: u8 = 0x1;
/// Channel-select register.
const ADC_USR_CH_SEL_CTL: u16 = ADC_BASE + 0x44;
/// Enable control 1; bit 7 is ADC enable.
const ADC_USR_EN_CTL1: u16 = ADC_BASE + 0x46;
/// ADC enable bit.
const ADC_USR_EN_CTL1_ADC_EN: u8 = 0x80;
/// Conversion-request register; bit 7 requests a conversion.
const ADC_USR_CONV_REQ: u16 = ADC_BASE + 0x47;
/// Conversion-request bit.
const ADC_USR_CONV_REQ_REQ: u8 = 0x80;
/// Result data, low byte then high byte.
const ADC_USR_DATA0: u16 = ADC_BASE + 0x50;
const ADC_USR_DATA1: u16 = ADC_BASE + 0x51;

/// The `VPH_PWR` (phone power rail) channel number. The
/// battery voltage is read from this channel: there is no
/// `vbat_sns` channel in this device's ADC device tree (the
/// `vadc@3100` node defines `vph_pwr` at `reg = <0x83>`, not a
/// `vbat_sns` entry), and VPH is the main power rail, which is
/// the battery voltage when the phone runs on battery. The
/// `3` in the C driver's `ADC_CHAN_VOLT("vbat_sns", 3, ...)`
/// is the prescale *index*, not a channel number -- writing 3
/// to the channel select converts an undefined channel and
/// returns a meaningless value (about half the real voltage).
const VBATT_CHANNEL: u8 = 0x83;
/// Poll delay between EOC checks, in microseconds (the C
/// driver's `ADC_POLL_DELAY_MIN_US` / `MAX_US`).
const ADC_POLL_DELAY_MIN_US: u64 = 10000;
const ADC_POLL_DELAY_MAX_US: u64 = 10001;
/// Maximum EOC polls before giving up (the C driver's
/// `ADC_CONV_TIME_RETRY_POLL`).
const ADC_CONV_TIME_RETRY_POLL: u32 = 40;
/// ADC reference voltage, in millivolts.
const ADC_VDD_REF_MV: u64 = 1875;
/// Full-scale code for voltage (`data_pmic_rev2`).
const ADC_FULL_SCALE_CODE_VOLT: u64 = 0x4000;
/// VBATT prescale: the VPH rail is divided by 3 before the
/// ADC (the device tree gives `vph_pwr` a `qcom,pre-scaling`
/// of `<0x01 0x03>`, ratio 1/3, so multiply the result by 3).
const VBATT_PRESCALE_NUM: u64 = 1;
const VBATT_PRESCALE_DEN: u64 = 3;

/// Shared device state: the SPMI handle to the PMIC.
#[pin_data]
struct Shared {
    sdev: SpmiDevice,
}

impl Shared {
    /// Read one ADC register (a single byte).
    fn read_reg(&self, addr: u16) -> Result<u8> {
        self.sdev.read_u8(addr)
    }

    /// Write one ADC register (a single byte).
    fn write_reg(&self, addr: u16, val: u8) -> Result<()> {
        self.sdev.write_u16(addr, val)
    }

    /// Run one VBATT conversion and return `(voltage_uV, raw)`.
    /// Selects channel 3, enables the ADC (read-modify-write),
    /// requests a conversion, polls for EOC, then reads the
    /// 16-bit result.
    fn read_vbatt(&self) -> Result<(u32, u16)> {
        // Select the VBAT_SNS channel.
        self.write_reg(ADC_USR_CH_SEL_CTL, VBATT_CHANNEL)?;
        // Enable the ADC, preserving the C driver's other
        // enable-control bits.
        let en = self.read_reg(ADC_USR_EN_CTL1)?;
        self.write_reg(ADC_USR_EN_CTL1, en | ADC_USR_EN_CTL1_ADC_EN)?;
        // Request a conversion.
        self.write_reg(ADC_USR_CONV_REQ, ADC_USR_CONV_REQ_REQ)?;
        // Poll for end-of-conversion.
        let mut eoc = false;
        for _ in 0..ADC_CONV_TIME_RETRY_POLL {
            let status = self.read_reg(ADC_USR_STATUS1)?;
            if status & ADC_USR_STATUS1_REQ_STS_EOC_MASK == ADC_USR_STATUS1_EOC {
                eoc = true;
                break;
            }
            // SAFETY: `usleep_range` is the documented sleep;
            // the range is the C driver's poll delay.
            unsafe {
                bindings::usleep_range(ADC_POLL_DELAY_MIN_US, ADC_POLL_DELAY_MAX_US);
            }
        }
        if !eoc {
            // The ADC never signalled end-of-conversion. There
            // is no `ETIMEDOUT` in `error::code`, so this is
            // reported as an I/O error.
            return Err(EIO);
        }
        // Read the 16-bit result, low byte first.
        let lsb = self.read_reg(ADC_USR_DATA0)? as u16;
        let msb = self.read_reg(ADC_USR_DATA1)? as u16;
        let raw: u16 = (msb << 8) | lsb;
        // Scale to microvolts (the C driver's
        // `qcom_vadc_scale_hw_calib_volt`): `uV = raw * 1875mV
        // * 1000 / full_scale * prescale_den / prescale_num`.
        // The intermediate needs 64 bits (raw * 1875 * 1000
        // exceeds u32).
        let mut uv: u64 = raw as u64;
        uv = uv * ADC_VDD_REF_MV * 1000;
        uv = uv / ADC_FULL_SCALE_CODE_VOLT;
        uv = uv * VBATT_PRESCALE_DEN;
        uv = uv / VBATT_PRESCALE_NUM;
        Ok((uv as u32, raw))
    }

    /// Render one snapshot: the battery voltage in microvolts
    /// and millivolts, plus the raw ADC code. On a conversion
    /// failure the voltage reads 0 and the raw code 0.
    fn snapshot(&self) -> [u8; 64] {
        let mut buf = [0u8; 64];
        let mut len = 0usize;
        let (uv, raw) = self.read_vbatt().unwrap_or((0, 0));
        len += copy_bytes(&mut buf[len..], b"vbatt_uv=");
        len += write_u32_dec(&mut buf[len..], uv);
        len += copy_bytes(&mut buf[len..], b"\nvbatt_mv=");
        len += write_u32_dec(&mut buf[len..], uv / 1000);
        len += copy_bytes(&mut buf[len..], b"\nraw_code=0x");
        len += write_u32_hex(&mut buf[len..], raw as u32);
        len += copy_bytes(&mut buf[len..], b"\n");
        buf
    }

    /// The length of the snapshot text (up to the first NUL).
    fn snapshot_len(buf: &[u8; 64]) -> usize {
        buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
    }
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct BatteryOps {
    shared: Arc<Shared>,
}

impl FileOperations for BatteryOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // A live snapshot, not a stream: every read runs a
        // fresh conversion. Honour the offset so `cat` sees EOF.
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
struct RustBatteryDriver {
    misc: Option<MiscRegistration<BatteryOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustBatteryDriver {}
unsafe impl Sync for RustBatteryDriver {}

impl PlatformDriver for RustBatteryDriver {
    const NAME: &'static CStr = c_str!("rust_battery");

    fn probe(&mut self, _pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_battery: probe, opening SPMI handle to the PMIC\n");

        // A direct SPMI handle to the PMIC (slave 0). The ADC
        // itself already has a C driver, so no Rust SPMI driver
        // can bind to it; this handle bypasses binding.
        let sdev = SpmiDevice::for_slave(0)?;
        // One conversion as the probe-time proof that the ADC
        // is readable.
        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared { sdev }))?
            .into();
        let (uv, raw) = shared.read_vbatt()?;
        pr_info!("rust_battery: vbatt = {} uV (raw {:#x})\n", uv, raw);

        let misc = MiscRegistration::<BatteryOps>::new(
            c_str!("rust_battery"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_battery: registered /dev/rust_battery\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_battery: remove, deregistering /dev/rust_battery\n");
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform driver registration plus the
/// synthetic platform device it binds to. Kept alive for the
/// module's lifetime; dropping it unregisters the driver (and
/// so runs `remove` on the bound device).
struct RustBattery {
    _reg: platform::Registration<RustBatteryDriver>,
    _dev: platform::Device,
}

impl kernel::Module for RustBattery {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_battery: registering platform driver\n");
        let drv = RustBatteryDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustBatteryDriver>::register(drv, module.as_ptr())?;
        // A synthetic platform device for the driver to bind
        // to (the ADC itself is an SPMI device, already owned
        // by its C driver).
        let dev = platform::Device::create(c_str!("rust_battery"), -1)?;
        Ok(Self { _reg: reg, _dev: dev })
    }
}
