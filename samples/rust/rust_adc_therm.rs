// SPDX-License-Identifier: GPL-2.0

//! PMIC ADC thermistor reader (Step 15).
//!
//! Reads the board thermistor channels straight from the
//! PMIC ADC and serves them on `/dev/rust_adc_therm`:
//!
//! ```text
//! xo_therm_raw=0x1234
//! xo_therm_uv=1234567
//! msm_therm_raw=0x1234
//! msm_therm_uv=1234567
//! emmc_therm_raw=0x1234
//! emmc_therm_uv=1234567
//! pa_therm0_raw=0x1234
//! pa_therm0_uv=1234567
//! ```
//!
//! The channels are the ratiometric thermistor inputs of
//! `vadc@3100` (`qcom,spmi-adc-rev2`): `xo_therm` (0x4c),
//! `msm_therm` (0x4d), `emmc_therm` (0x4e), `pa_therm0`
//! (0x4f). All use pre-scaling 1/1, so the voltage is
//! `uV = raw * 1875mV * 1000 / 0x4000`. The raw codes prove
//! the ADC conversion works on these channels; converting a
//! thermistor voltage to Celsius needs the thermistor table
//! the C driver carries, so this driver reports the voltage
//! and leaves the temperature conversion to the reader.
//! The conversion follows the same sequence as the battery
//! driver (select, enable, request, poll EOC, read result),
//! through a direct SPMI handle to the PMIC. The ADC already
//! has a C driver, so no Rust SPMI driver can bind to it;
//! the direct handle bypasses binding. Every read is
//! read-only.

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
    type: RustAdcTherm,
    name: "rust_adc_therm",
    author: "MayaKernel",
    description: "Rust PMIC ADC thermistor reader (spmi)",
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

/// Poll delay between EOC checks, in microseconds.
const ADC_POLL_DELAY_MIN_US: u64 = 10000;
const ADC_POLL_DELAY_MAX_US: u64 = 10001;
/// Maximum EOC polls before giving up.
const ADC_CONV_TIME_RETRY_POLL: u32 = 40;
/// ADC reference voltage, in millivolts.
const ADC_VDD_REF_MV: u64 = 1875;
/// Full-scale code for voltage (`data_pmic_rev2`).
const ADC_FULL_SCALE_CODE_VOLT: u64 = 0x4000;

/// Thermistor channels (name, channel number). All use
/// pre-scaling 1/1.
const THERM_CHANNELS: [(&str, u8); 4] = [
    ("xo_therm", 0x4c),
    ("msm_therm", 0x4d),
    ("emmc_therm", 0x4e),
    ("pa_therm0", 0x4f),
];

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

    /// Run one conversion on `channel` and return
    /// `(voltage_uV, raw)`. Same sequence as the battery
    /// driver, but the prescale is 1/1 for these channels.
    fn read_channel(&self, channel: u8) -> Result<(u32, u16)> {
        // Select the channel.
        self.write_reg(ADC_USR_CH_SEL_CTL, channel)?;
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
            return Err(EIO);
        }
        // Read the 16-bit result, low byte first.
        let lsb = self.read_reg(ADC_USR_DATA0)? as u16;
        let msb = self.read_reg(ADC_USR_DATA1)? as u16;
        let raw: u16 = (msb << 8) | lsb;
        // Scale to microvolts, prescale 1/1.
        let mut uv: u64 = raw as u64;
        uv = uv * ADC_VDD_REF_MV * 1000;
        uv = uv / ADC_FULL_SCALE_CODE_VOLT;
        Ok((uv as u32, raw))
    }

    /// Render one snapshot: raw code and microvolts for each
    /// thermistor channel. On a conversion failure the channel
    /// reports 0.
    fn snapshot(&self) -> [u8; 256] {
        let mut buf = [0u8; 256];
        let mut len = 0usize;
        for (name, channel) in THERM_CHANNELS {
            let (uv, raw) = self.read_channel(channel).unwrap_or((0, 0));
            len += copy_bytes(&mut buf[len..], name.as_bytes());
            len += copy_bytes(&mut buf[len..], b"_raw=0x");
            len += write_u32_hex(&mut buf[len..], raw as u32);
            len += copy_bytes(&mut buf[len..], b"\n");
            len += copy_bytes(&mut buf[len..], name.as_bytes());
            len += copy_bytes(&mut buf[len..], b"_uv=");
            len += write_u32_dec(&mut buf[len..], uv);
            len += copy_bytes(&mut buf[len..], b"\n");
        }
        buf
    }

    /// The length of the snapshot text (up to the first NUL).
    fn snapshot_len(buf: &[u8; 256]) -> usize {
        buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
    }
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct AdcThermOps {
    shared: Arc<Shared>,
}

impl FileOperations for AdcThermOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // A live snapshot, not a stream: every read runs fresh
        // conversions. Honour the offset so `cat` sees EOF.
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
struct RustAdcThermDriver {
    misc: Option<MiscRegistration<AdcThermOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustAdcThermDriver {}
unsafe impl Sync for RustAdcThermDriver {}

impl PlatformDriver for RustAdcThermDriver {
    const NAME: &'static CStr = c_str!("rust_adc_therm");

    fn probe(&mut self, _pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_adc_therm: probe, opening SPMI handle to the PMIC\n");

        // A direct SPMI handle to the PMIC (slave 0). The ADC
        // itself already has a C driver, so no Rust SPMI driver
        // can bind to it; this handle bypasses binding.
        let sdev = SpmiDevice::for_slave(0)?;
        // One conversion as the probe-time proof that the ADC
        // is readable on a thermistor channel.
        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared { sdev }))?
            .into();
        let (uv, raw) = shared.read_channel(THERM_CHANNELS[0].1)?;
        pr_info!(
            "rust_adc_therm: {} = {} uV (raw {:#x})\n",
            THERM_CHANNELS[0].0, uv, raw
        );

        let misc = MiscRegistration::<AdcThermOps>::new(
            c_str!("rust_adc_therm"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_adc_therm: registered /dev/rust_adc_therm\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_adc_therm: remove, deregistering /dev/rust_adc_therm\n");
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform driver registration plus the
/// synthetic platform device it binds to. Kept alive for the
/// module's lifetime; dropping it unregisters the driver (and
/// so runs `remove` on the bound device).
struct RustAdcTherm {
    _reg: platform::Registration<RustAdcThermDriver>,
    _dev: platform::Device,
}

impl kernel::Module for RustAdcTherm {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_adc_therm: registering platform driver\n");
        let drv = RustAdcThermDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustAdcThermDriver>::register(drv, module.as_ptr())?;
        // A synthetic platform device for the driver to bind
        // to (the ADC itself is an SPMI device, already owned
        // by its C driver).
        let dev = platform::Device::create(c_str!("rust_adc_therm"), -1)?;
        Ok(Self { _reg: reg, _dev: dev })
    }
}
