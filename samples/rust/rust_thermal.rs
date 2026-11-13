// SPDX-License-Identifier: GPL-2.0

//! Thermal sensor reader (Step 10, option 1).
//!
//! Reads the SoC temperature straight from the tsens thermal
//! monitor (TM) registers, the same way the C `tsens-v2` driver
//! does, and serves it on `/dev/rust_thermal`:
//!
//! ```text
//! temp_milli_c=56400
//! raw_code=0x234
//! valid=1
//! s0=56400
//! s1=51200
//! ...
//! ```
//!
//! The TM region is `tsens_tm_physical` (`0x10ae000`, 0x200) from
//! the `tsens@10ad000` node (`qcom,sdm660-tsens`). Each sensor's
//! status register is at `TM + 0xa0 + hw_id * 4`
//! (`STATUS_OFFSET` in `tsens-v2.c`). The low 12 bits are the
//! temperature code in deciCelsius (tenths of a degree); bit 21
//! is the valid bit. The C driver converts with
//! `sign_extend32(code, 11) * 100` to get milliCelsius, and this
//! driver does the same. The read is read-only (`readl` on a
//! status register), so it cannot disturb the C tsens driver
//! that owns the device; the value can be cross-checked against
//! the matching `/sys/class/thermal/thermal_zone*/temp`.
//!
//! The 14 sensors are reported by client index, using the
//! `qcom,sensor-id` mapping from the device tree (client ->
//! hw_id): 0->0, 1->10, 2->11, 3->4, 4->5, 5->6, 6->7, 7->8,
//! 8->13, 9->2, 10->3, 11->12, 12->9, 13->1. `temp_milli_c`,
//! `raw_code` and `valid` are sensor 0 (the CPU sensor) for
//! backwards compatibility; `s0`..`s13` give every sensor.

use kernel::{
    bindings,
    c_str,
    file::{File, FileOperations, UserSliceWriter},
    io::IoMem,
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    sync::{Arc, UniqueArc},
};
use kernel::text::{copy_bytes, write_u32_dec, write_i32_dec, write_u32_hex};

module! {
    type: RustThermal,
    name: "rust_thermal",
    author: "MayaKernel",
    description: "Rust thermal sensor reader (tsens-v2)",
    license: "GPL",
}

/// TM region base and size (`tsens@10ad000`, `tsens_tm_physical`).
const TSENS_TM_BASE: u64 = 0x10ae_000;
const TSENS_TM_SIZE: usize = 0x200;
/// Status register offset (`STATUS_OFFSET` in `tsens-v2.c`).
const STATUS_OFFSET: usize = 0xa0;
/// Low 12 bits hold the temperature code (`LAST_TEMP_MASK`).
const LAST_TEMP_MASK: u32 = 0xfff;
/// Bit 21 of the status register: the reading is valid.
const STATUS_VALID_BIT: u32 = 1 << 21;
/// Hardware id of sensor 0 (client 0, the CPU sensor).
const SENSOR0_HW_ID: usize = 0;

/// The 14 sensors' hardware ids, by client index, from the
/// device tree's `qcom,sensor-id` property (client -> hw_id).
const SENSOR_HW_IDS: [usize; 14] = [0, 10, 11, 4, 5, 6, 7, 8, 13, 2, 3, 12, 9, 1];

/// Shared device state: the mapped TM registers.
#[pin_data]
struct Shared {
    iomem: IoMem,
}

impl Shared {
    /// Read one sensor's status register: (temperature in
    /// milliCelsius, raw 12-bit code, valid bit).
    fn read_sensor(&self, hw_id: usize) -> (i32, u32, bool) {
        let code = self.iomem.readl(STATUS_OFFSET + hw_id * 4);
        let raw = code & LAST_TEMP_MASK;
        let valid = (code & STATUS_VALID_BIT) != 0;
        // Sign-extend the 12-bit code (bit 11 is the sign) and
        // convert deciCelsius to milliCelsius, as `tsens-v2.c`
        // does: `sign_extend32(code, 11) * 100`.
        let milli_c = sign_extend_12(raw) as i32 * 100;
        (milli_c, raw, valid)
    }

    /// Render one snapshot: sensor 0's temperature, raw code and
    /// valid bit (the original three lines), then every sensor's
    /// temperature as `s<client>=<milliC>`.
    fn snapshot(&self) -> [u8; 256] {
        let mut buf = [0u8; 256];
        let mut len = 0usize;
        let (milli_c, raw, valid) = self.read_sensor(SENSOR0_HW_ID);
        len += copy_bytes(&mut buf[len..], b"temp_milli_c=");
        len += write_i32_dec(&mut buf[len..], milli_c);
        len += copy_bytes(&mut buf[len..], b"\nraw_code=0x");
        len += write_u32_hex(&mut buf[len..], raw);
        len += copy_bytes(&mut buf[len..], b"\nvalid=");
        len += write_u32_dec(&mut buf[len..], valid as u32);
        len += copy_bytes(&mut buf[len..], b"\n");
        for (client, &hw_id) in SENSOR_HW_IDS.iter().enumerate() {
            let (milli_c, _, _) = self.read_sensor(hw_id);
            len += copy_bytes(&mut buf[len..], b"s");
            len += write_u32_dec(&mut buf[len..], client as u32);
            len += copy_bytes(&mut buf[len..], b"=");
            len += write_i32_dec(&mut buf[len..], milli_c);
            len += copy_bytes(&mut buf[len..], b"\n");
        }
        buf
    }

    /// The length of the snapshot text (up to the first NUL).
    fn snapshot_len(buf: &[u8; 256]) -> usize {
        buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
    }
}

/// Sign-extend a 12-bit value (bit 11 is the sign bit).
fn sign_extend_12(v: u32) -> i32 {
    let v = v & LAST_TEMP_MASK;
    if v & (1 << 11) != 0 {
        (v | !LAST_TEMP_MASK) as i32
    } else {
        v as i32
    }
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct ThermalOps {
    shared: Arc<Shared>,
}

impl FileOperations for ThermalOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // A live snapshot, not a stream: every read re-reads
        // the sensor. Honour the offset so `cat` sees EOF.
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
struct RustThermalDriver {
    misc: Option<MiscRegistration<ThermalOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustThermalDriver {}
unsafe impl Sync for RustThermalDriver {}

impl PlatformDriver for RustThermalDriver {
    const NAME: &'static CStr = c_str!("rust_thermal");

    fn probe(&mut self, pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_thermal: probe, driver bound\n");

        // Map the TM region and read sensor 0 once, as the
        // probe-time proof that the register is readable.
        let iomem = IoMem::map(TSENS_TM_BASE, TSENS_TM_SIZE)?;
        let code = iomem.readl(STATUS_OFFSET + SENSOR0_HW_ID * 4);
        let raw = code & LAST_TEMP_MASK;
        let valid = (code & STATUS_VALID_BIT) != 0;
        let milli_c = sign_extend_12(raw) as i32 * 100;
        pr_info!(
            "rust_thermal: sensor0 raw = {:#x}, valid = {}, temp = {} mC\n",
            raw, valid, milli_c
        );

        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared { iomem }))?
            .into();
        let misc = MiscRegistration::<ThermalOps>::new(
            c_str!("rust_thermal"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_thermal: registered /dev/rust_thermal\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_thermal: remove, deregistering /dev/rust_thermal\n");
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform device and the driver registration.
/// Kept alive for the module's lifetime; dropping them unregisters
/// the device (and so runs `remove`) and the driver.
struct RustThermal {
    _dev: platform::Device,
    _reg: platform::Registration<RustThermalDriver>,
}

impl kernel::Module for RustThermal {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_thermal: registering platform driver\n");
        let drv = RustThermalDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustThermalDriver>::register(drv, module.as_ptr())?;
        pr_info!("rust_thermal: creating platform device\n");
        let dev = platform::Device::create(c_str!("rust_thermal"), -1)?;
        Ok(Self { _dev: dev, _reg: reg })
    }
}
