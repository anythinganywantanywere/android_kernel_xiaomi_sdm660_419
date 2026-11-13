// SPDX-License-Identifier: GPL-2.0

//! Rust platform driver sample.
//!
//! Registers a platform driver named `rust_platform` and creates a
//! matching platform device, so `probe` runs at boot without any
//! device-tree change (the platform bus falls back to matching by
//! name). `probe` only logs: the `/dev/rust_platform` misc device
//! is registered in `init` (which owns the module pointer the misc
//! registration needs) and deregistered in `remove`, so the driver
//! lifecycle still owns it. A real driver would register its
//! sub-devices inside `probe` instead; doing that here would need
//! the module pointer plumbed through the platform trait.
//!
//! Step 3 (iomem): the device carries one `IORESOURCE_MEM`
//! resource, the GIC distributor (SDM660/636: `0x17a00000`, from
//! `arch/arm64/boot/dts/vendor/qcom/sdm660.dtsi`). `probe` maps it
//! with [`IoMem`] and reads the distributor type register
//! (`GICD_TYPER`, offset `0x004`) -- a read-only hardware register,
//! so the proof never writes to the hardware. The mapping is held
//! for the driver's lifetime and unmapped in `remove`.
//!
//! Steps 4-7 (irq, clk, gpio, regulator): `probe` also proves the
//! four resource abstractions, and holds them for the driver's
//! lifetime:
//!
//! * irq: a virtual IRQ is allocated and requested; writing to
//!   `/dev/rust_platform` fires it in software (`generic_handle_irq`)
//!   and the handler counts it.
//! * clk: the `xo_board` fixed clock (19.2 MHz) is enabled and its
//!   rate read, then disabled again.
//! * gpio: the camera-focus key (TLMM GPIO 64, an input) is
//!   resolved through the device tree and read, and an unused
//!   TLMM GPIO (100, absent from the whole device tree) is
//!   driven as an output and read back. The DT numbers are
//!   hardware offsets, not Linux GPIO numbers: the TLMM base
//!   is derived from the resolved key, and the test pin is the
//!   same base plus its own offset. The TLMM has only 114
//!   pins (0-113), so the test pin must stay in that range.
//! * regulator: the `pm660_l1` regulator is read (voltage and
//!   enabled state) and, when it is already on, enabled and
//!   disabled once (a balanced refcount, no hardware change).

use core::sync::atomic::{AtomicU32, Ordering};
use kernel::{
    bindings,
    c_str,
    clk::Clk,
    file::{File, FileOperations, UserSliceReader, UserSliceWriter},
    gpio::{self, Gpio},
    io::IoMem,
    irq::{self, Irq, IrqHandler, IrqReturn, Virq},
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    regulator::Regulator,
    sync::{new_mutex, Arc, Mutex, UniqueArc},
};

module! {
    type: RustPlatform,
    name: "rust_platform",
    author: "MayaKernel",
    description: "Rust platform driver sample",
    license: "GPL",
}

/// Greeting returned by `read` on a fresh device.
const GREETING: &[u8] = b"rust-platform v1\n";

/// Maximum bytes kept in the device buffer.
const MAX_BUF: usize = 128;

/// GIC distributor base, SDM660/636 (`sdm660.dtsi`: `intc` node,
/// `reg = <0x17a00000 0x10000>`).
const GICD_BASE: u64 = 0x17a0_0000;

/// GIC distributor size, from the same `reg` property.
const GICD_SIZE: usize = 0x10000;

/// GIC distributor type register offset (`GICD_TYPER`, read-only).
const GICD_TYPER: usize = 0x004;

/// Camera-focus key, TLMM GPIO 64 (`sdm660.dtsi`: `gpio_keys`,
/// `gpios = <&tlmm 64 0x1>`). An input, owned by `gpio-keys`;
/// only read, never reconfigured.
const GPIO_KEY_FOCUS: u32 = 64;

/// Unused TLMM GPIO, used as a test output. Absent from the whole
/// compiled device tree (`sdm636-mtp-whyred.dtb`), so it drives
/// nothing on this board. The TLMM only has 114 pins (0-113,
/// `ngpios = 114` in `pinctrl-sdm660.c`), so the number must
/// stay in that range -- the earlier 120 was not a pin at all.
const GPIO_TEST_OUT: u32 = 100;

/// Fixed clock node name (`sdm660.dtsi`: `clocks` node).
const XO_NODE: &CStr = c_str!("xo_board");

/// Regulator name (`pm660-rpm-regulator.dtsi`, enabled in the
/// compiled device tree).
const PM660_L1: &CStr = c_str!("pm660_l1");

struct RustPlatform {
    _dev: platform::Device,
    _reg: platform::Registration<RustPlatformDriver>,
}

/// The platform driver. Owns the misc device, so `remove`
/// deregisters `/dev/rust_platform`, and the GIC mapping, the
/// virtual IRQ and its request, the clock, the test GPIO and the
/// regulator, so `remove` releases them all.
///
/// The `irq` field is declared before `virq`: fields drop in
/// declaration order, and the request must be freed before the
/// descriptor.
struct RustPlatformDriver {
    misc: Option<MiscRegistration<RustPlatformOps>>,
    iomem: Option<IoMem>,
    shared: Arc<Shared>,
    owner: *mut bindings::module,
    irq: Option<Irq<IrqCounter>>,
    virq: Option<Virq>,
    clk: Option<Clk>,
    gpio_out: Option<Gpio>,
    reg: Option<Regulator>,
}

// SAFETY: `owner` is only ever passed to `__irq_alloc_descs`
// (which stores it, never dereferences it from Rust); every other
// field is `Send + Sync` on its own. Sharing the driver state
// across the interrupt context (the IRQ handler) is sound because
// the handler only touches the `IrqCounter`'s atomics.
unsafe impl Send for RustPlatformDriver {}
unsafe impl Sync for RustPlatformDriver {}

/// Device-wide buffer for `/dev/rust_platform`.
///
/// Same pattern as the Step 1 sample, kept local: samples are
/// separate crates and cannot share each other's types.
#[pin_data]
struct Shared {
    #[pin]
    buf: Mutex<Vec<u8>>,
    /// Virtual IRQ number, set by `probe` (`u32::MAX` = none).
    /// A `write` fires it, so the handler runs and counts.
    /// (`u32::MAX`, not 0, is the sentinel: the virtual IRQ
    /// number can itself be 0 -- the first free descriptor.)
    irq: AtomicU32,
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct RustPlatformOps {
    shared: Arc<Shared>,
}

impl FileOperations for RustPlatformOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        let buf = self.shared.buf.lock();
        let data: &[u8] = if buf.is_empty() { GREETING } else { &buf };
        let offset = offset as usize;
        if offset >= data.len() {
            return Ok(0);
        }
        writer.write(&data[offset..])
    }

    fn write(&self, _file: &File, reader: &mut UserSliceReader, _offset: u64) -> Result<usize> {
        let mut tmp = [0u8; MAX_BUF];
        let n = reader.read(&mut tmp)?;
        let mut buf = self.shared.buf.lock();
        buf.clear();
        for &b in &tmp[..n] {
            buf.push(b, GFP_KERNEL).map_err(|_| ENOMEM)?;
        }
        drop(buf);
        // Fire the virtual IRQ (if `probe` allocated one): the
        // handler runs synchronously and counts it. The outcome
        // is logged: a silent discard would hide a dispatch
        // failure (`generic_handle_irq` returning an error).
        let virq = self.shared.irq.load(Ordering::Relaxed);
        if virq != u32::MAX {
            match irq::fire(virq) {
                Ok(()) => pr_info!("rust_platform: fired irq {}\n", virq),
                Err(e) => {
                    pr_info!("rust_platform: fire irq {} failed: {}\n", virq, e.to_errno())
                }
            }
        }
        Ok(n)
    }
}

/// Counts how many times the virtual IRQ fired. Runs in interrupt
/// context, so the counter is an atomic (no sleeping lock).
struct IrqCounter {
    count: AtomicU32,
}

impl IrqHandler for IrqCounter {
    fn handle(&self, irq: u32) -> IrqReturn {
        let n = self.count.fetch_add(1, Ordering::Relaxed) + 1;
        pr_info!("rust_platform: irq {} fired, count {}\n", irq, n);
        IrqReturn::Handled
    }
}

impl PlatformDriver for RustPlatformDriver {
    const NAME: &'static CStr = c_str!("rust_platform");

    fn probe(&mut self, pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_platform: probe, driver bound\n");

        // Build every owned object as a local first, and only
        // assign them to `self` once all of them succeeded: on
        // any failure the locals drop in reverse order and free
        // what was already acquired (the IRQ before its
        // descriptor, the mapping last).
        let (phys, size) = pdev.mem_resource(0).ok_or(ENODEV)?;
        let iomem = IoMem::map(phys, size)?;
        let typer = iomem.readl(GICD_TYPER);
        pr_info!("rust_platform: GIC TYPER = {:#x}\n", typer);

        let virq = Virq::alloc(self.owner)?;
        irq::set_simple_handler(virq.number())?;
        let irq = Irq::request(
            virq.number(),
            IrqCounter {
                count: AtomicU32::new(0),
            },
            0,
            c_str!("rust_platform"),
        )?;
        pr_info!("rust_platform: virtual irq {}\n", virq.number());
        self.shared.irq.store(virq.number(), Ordering::Relaxed);

        let clk = Clk::of_get(XO_NODE)?;
        clk.enable()?;
        let rate = clk.rate();
        pr_info!("rust_platform: xo rate = {} Hz\n", rate);
        clk.disable();

        // Resolve the camera-focus key through the device tree:
        // the DT number is a hardware offset, not a Linux GPIO
        // number. The TLMM base follows from it, so the
        // unreferenced test pin is the same base plus its own
        // offset. (Returns `-EPROBE_DEFER` while the TLMM chip
        // is not registered yet; the core then retries `probe`.)
        let key_linux = gpio::number_from_of(c_str!("camera_focus"), c_str!("gpios"), 0)?;
        let key = Gpio::from_number(key_linux)?;
        let key_val = key.get_value()?;
        pr_info!(
            "rust_platform: gpio {} (camera focus, linux {}) = {}\n",
            GPIO_KEY_FOCUS, key_linux, key_val
        );

        let out_linux = key_linux + (GPIO_TEST_OUT - GPIO_KEY_FOCUS);
        let gpio_out = Gpio::from_number(out_linux)?;
        gpio_out.direction_output(false)?;
        gpio_out.set_value(true);
        let hi = gpio_out.get_value()?;
        gpio_out.set_value(false);
        let lo = gpio_out.get_value()?;
        pr_info!(
            "rust_platform: gpio {} out: hi = {}, lo = {}\n",
            GPIO_TEST_OUT, hi, lo
        );

        let reg = Regulator::get(PM660_L1)?;
        let uv = reg.voltage()?;
        let on = reg.is_enabled()?;
        pr_info!("rust_platform: pm660_l1 = {} uV, enabled = {}\n", uv, on);
        if on {
            reg.enable()?;
            reg.disable()?;
            pr_info!("rust_platform: pm660_l1 enable/disable balanced\n");
        }

        self.iomem = Some(iomem);
        self.irq = Some(irq);
        self.virq = Some(virq);
        self.clk = Some(clk);
        self.gpio_out = Some(gpio_out);
        self.reg = Some(reg);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_platform: remove, deregistering /dev/rust_platform\n");
        self.irq.take();
        self.virq.take();
        self.clk.take();
        self.reg.take();
        self.gpio_out.take();
        self.iomem.take();
        self.misc.take();
    }
}

impl kernel::Module for RustPlatform {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_platform: registering /dev/rust_platform\n");
        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared {
                buf <- new_mutex!(Vec::new(), "rust_platform_buf"),
                irq: AtomicU32::new(u32::MAX),
            }))?
            .into();
        let misc = MiscRegistration::<RustPlatformOps>::new(
            c_str!("rust_platform"),
            DYNAMIC_MINOR,
            shared.clone(),
            module.as_ptr(),
        )?;
        let drv = RustPlatformDriver {
            misc: Some(misc),
            iomem: None,
            shared,
            owner: module.as_ptr(),
            irq: None,
            virq: None,
            clk: None,
            gpio_out: None,
            reg: None,
        };
        pr_info!("rust_platform: registering platform driver\n");
        let reg = platform::Registration::<RustPlatformDriver>::register(drv, module.as_ptr())?;
        pr_info!("rust_platform: creating platform device\n");
        let gic = bindings::resource {
            start: GICD_BASE,
            end: GICD_BASE + GICD_SIZE as u64 - 1,
            name: c_str!("gic").as_ptr(),
            flags: bindings::IORESOURCE_MEM as core::ffi::c_ulong,
            ..Default::default()
        };
        let dev = platform::Device::create_with_resources(
            c_str!("rust_platform"),
            -1,
            &[gic],
        )?;
        Ok(Self { _dev: dev, _reg: reg })
    }
}
