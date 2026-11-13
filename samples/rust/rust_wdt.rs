// SPDX-License-Identifier: GPL-2.0

//! MSM watchdog status reader (Step 16).
//!
//! Reads the watchdog's enable state, status and timeouts
//! straight from its registers and serves them on
//! `/dev/rust_wdt`:
//!
//! ```text
//! wdt_en=1
//! wdt_sts=0
//! wdt_bark_time=11000
//! wdt_bite_time=20000
//! ```
//!
//! The watchdog is `qcom,wdt@17817000`
//! (`qcom,msm-watchdog`), a memory-mapped block (not SPMI),
//! so it is read through `IoMem` like the GIC in Step 3.
//! Register offsets are from `drivers/soc/qcom/watchdog_v2.c`
//! (`WDT0_EN = 0x08`, `WDT0_STS = 0x0C`, `WDT0_BARK_TIME =
//! 0x10`, `WDT0_BITE_TIME = 0x14`): the `qcom,msm-watchdog`
//! is driven by `watchdog_v2`, not by `qcom-wdt.c`, whose
//! APCS offsets (`0x40`/`0x44`/`0x4C`/`0x5C`) read zero here.
//! Every read here is a plain `readl`: reading never pets
//! the watchdog and never changes its configuration, so the
//! device cannot be reset by this driver. The watchdog
//! already has a C driver; this driver only observes it.

use kernel::{
    bindings,
    c_str,
    file::{File, FileOperations, UserSliceWriter},
    io::IoMem,
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    sync::{Arc, UniqueArc},
    text::{copy_bytes, write_u32_dec},
};

module! {
    type: RustWdt,
    name: "rust_wdt",
    author: "MayaKernel",
    description: "Rust MSM watchdog status reader (iomem)",
    license: "GPL",
}

/// Watchdog base (`qcom,wdt@17817000`, `reg = <0x17817000 0x1000>`).
const WDT_BASE: u64 = 0x1781_7000;
/// Mapped region size.
const WDT_SIZE: usize = 0x1000;
/// Enable register offset (`WDT0_EN`, watchdog_v2 / KPSS).
const WDT_EN: u32 = 0x08;
/// Status register offset (`WDT0_STS`).
const WDT_STS: u32 = 0x0C;
/// Bark-timeout register offset (`WDT0_BARK_TIME`).
const WDT_BARK_TIME: u32 = 0x10;
/// Bite-timeout register offset (`WDT0_BITE_TIME`).
const WDT_BITE_TIME: u32 = 0x14;

/// Shared device state: the mapped watchdog registers.
#[pin_data]
struct Shared {
    iomem: IoMem,
}

impl Shared {
    /// Read one watchdog register (a 32-bit `readl`).
    fn read(&self, offset: u32) -> u32 {
        // SAFETY: the offset is within the mapped region and
        // the register is a plain 32-bit read-only view; the
        // watchdog is never written.
        self.iomem.readl(offset as usize)
    }

    /// Render one snapshot: enable, status, bark and bite
    /// timeouts.
    fn snapshot(&self) -> [u8; 128] {
        let mut buf = [0u8; 128];
        let mut len = 0usize;
        len += copy_bytes(&mut buf[len..], b"wdt_en=");
        len += write_u32_dec(&mut buf[len..], self.read(WDT_EN));
        len += copy_bytes(&mut buf[len..], b"\nwdt_sts=");
        len += write_u32_dec(&mut buf[len..], self.read(WDT_STS));
        len += copy_bytes(&mut buf[len..], b"\nwdt_bark_time=");
        len += write_u32_dec(&mut buf[len..], self.read(WDT_BARK_TIME));
        len += copy_bytes(&mut buf[len..], b"\nwdt_bite_time=");
        len += write_u32_dec(&mut buf[len..], self.read(WDT_BITE_TIME));
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
struct WdtOps {
    shared: Arc<Shared>,
}

impl FileOperations for WdtOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // A live snapshot, not a stream: every read re-reads
        // the registers. Honour the offset so `cat` sees EOF.
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
struct RustWdtDriver {
    misc: Option<MiscRegistration<WdtOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustWdtDriver {}
unsafe impl Sync for RustWdtDriver {}

impl PlatformDriver for RustWdtDriver {
    const NAME: &'static CStr = c_str!("rust_wdt");

    fn probe(&mut self, _pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_wdt: probe, mapping watchdog registers\n");

        let iomem = IoMem::map(WDT_BASE, WDT_SIZE)?;
        // One read as the probe-time proof that the registers
        // are readable.
        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared { iomem }))?
            .into();
        let en = shared.read(WDT_EN);
        pr_info!("rust_wdt: wdt_en = {}\n", en);

        let misc = MiscRegistration::<WdtOps>::new(
            c_str!("rust_wdt"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_wdt: registered /dev/rust_wdt\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_wdt: remove, deregistering /dev/rust_wdt\n");
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform driver registration plus the
/// synthetic platform device it binds to. Kept alive for the
/// module's lifetime; dropping it unregisters the driver (and
/// so runs `remove` on the bound device).
struct RustWdt {
    _reg: platform::Registration<RustWdtDriver>,
    _dev: platform::Device,
}

impl kernel::Module for RustWdt {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_wdt: registering platform driver\n");
        let drv = RustWdtDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustWdtDriver>::register(drv, module.as_ptr())?;
        // A synthetic platform device for the driver to bind
        // to (the watchdog is a platform device, already owned
        // by its C driver).
        let dev = platform::Device::create(c_str!("rust_wdt"), -1)?;
        Ok(Self { _reg: reg, _dev: dev })
    }
}
