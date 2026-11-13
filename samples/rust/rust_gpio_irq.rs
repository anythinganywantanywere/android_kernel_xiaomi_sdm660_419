// SPDX-License-Identifier: GPL-2.0

//! Real-hardware IRQ driver (Step 9, option 1).
//!
//! Steps 4-7 proved the `irq` abstraction only against a *virtual*
//! IRQ, fired on demand by `generic_handle_irq`. This driver proves
//! it against a real hardware interrupt line: the camera-focus
//! key's GPIO (TLMM 64, Linux 1230). The C `gpio-keys` driver owns
//! that key and already requested its IRQ with `IRQF_SHARED` (it
//! sets the flag whenever the button cannot be disabled, and this
//! key has no `can-disable` property), so this driver requests the
//! same line with `IRQF_SHARED` and counts every time the line
//! fires. The count is served on `/dev/rust_gpio_irq`:
//!
//! ```text
//! irq=311
//! count=3
//! ```
//!
//! The handler runs in interrupt context, so the count is an
//! atomic; no sleeping lock is touched there. The line is shared
//! with `gpio-keys`, which keeps owning the key; this driver only
//! observes it.

use core::ffi::c_ulong;
use core::sync::atomic::{AtomicU32, Ordering};

use kernel::{
    bindings,
    c_str,
    file::{File, FileOperations, UserSliceWriter},
    gpio::{self, Gpio},
    irq::{Irq, IrqHandler, IrqReturn},
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    sync::{Arc, UniqueArc},
};
use kernel::text::{copy_bytes, write_u32_dec};

module! {
    type: RustGpioIrq,
    name: "rust_gpio_irq",
    author: "MayaKernel",
    description: "Rust GPIO IRQ counter (shared camera-focus line)",
    license: "GPL",
}

/// Camera-focus GPIO node and property (same as Steps 4-7).
const GPIO_KEY_FOCUS_NODE: &CStr = c_str!("camera_focus");
const GPIO_KEY_FOCUS_PROP: &CStr = c_str!("gpios");

/// Shared device state: the IRQ number and how many times the
/// line has fired. The counter is atomic because the handler runs
/// in interrupt context.
#[pin_data]
struct Shared {
    irq: u32,
    count: AtomicU32,
}

/// The IRQ handler: bump the shared counter and claim the
/// interrupt. The line is shared with `gpio-keys`, whose handler
/// runs too; claiming it here only records that the line fired.
struct IrqCounter {
    shared: Arc<Shared>,
}

impl IrqHandler for IrqCounter {
    fn handle(&self, irq: u32) -> IrqReturn {
        let n = self.shared.count.fetch_add(1, Ordering::Relaxed) + 1;
        pr_info!("rust_gpio_irq: irq {} fired, count {}\n", irq, n);
        IrqReturn::Handled
    }
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct GpioIrqOps {
    shared: Arc<Shared>,
}

impl FileOperations for GpioIrqOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // One snapshot: `irq=<n>\ncount=<n>\n`. Honour the
        // offset so `cat` sees EOF instead of looping.
        let mut buf = [0u8; 64];
        let mut len = 0usize;
        len += copy_bytes(&mut buf[len..], b"irq=");
        len += write_u32_dec(&mut buf[len..], self.shared.irq);
        len += copy_bytes(&mut buf[len..], b"\ncount=");
        len += write_u32_dec(
            &mut buf[len..],
            self.shared.count.load(Ordering::Relaxed),
        );
        len += copy_bytes(&mut buf[len..], b"\n");
        let offset = offset as usize;
        if offset >= len {
            return Ok(0);
        }
        writer.write(&buf[offset..])
    }
}

/// Driver state. The misc device is registered in `probe`, so
/// `misc` and `shared` start empty; `irq` is requested in
/// `probe` too. The module pointer is the only field `init`
/// fills in.
struct RustGpioIrqDriver {
    misc: Option<MiscRegistration<GpioIrqOps>>,
    irq: Option<Irq<IrqCounter>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustGpioIrqDriver {}
unsafe impl Sync for RustGpioIrqDriver {}

impl PlatformDriver for RustGpioIrqDriver {
    const NAME: &'static CStr = c_str!("rust_gpio_irq");

    fn probe(&mut self, pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_gpio_irq: probe, driver bound\n");

        // Resolve the camera-focus GPIO (proven in Steps 4-7)
        // and ask the GPIO core for its interrupt number.
        let key_linux = gpio::number_from_of(GPIO_KEY_FOCUS_NODE, GPIO_KEY_FOCUS_PROP, 0)?;
        let key = Gpio::from_number(key_linux)?;
        let irq_num = key.to_irq()?;
        pr_info!(
            "rust_gpio_irq: gpio 64 (linux {}) irq = {}\n",
            key_linux, irq_num
        );

        // Shared state first, so the handler has something to
        // bump from the moment the line is requested.
        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared {
                irq: irq_num,
                count: AtomicU32::new(0),
            }))?
            .into();

        // Request the real hardware line. `gpio-keys` already
        // holds it with `IRQF_SHARED`, so share it; match its
        // trigger flags (rising and falling) too, or the IRQ
        // core logs a "Flags mismatch" warning for the line.
        let irq = Irq::request(
            irq_num,
            IrqCounter {
                shared: shared.clone(),
            },
            (bindings::IRQF_SHARED
                | bindings::IRQF_TRIGGER_RISING
                | bindings::IRQF_TRIGGER_FALLING) as c_ulong,
            c_str!("rust_gpio_irq"),
        )?;
        pr_info!("rust_gpio_irq: requested shared irq {}\n", irq_num);

        let misc = MiscRegistration::<GpioIrqOps>::new(
            c_str!("rust_gpio_irq"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_gpio_irq: registered /dev/rust_gpio_irq\n");

        self.shared = Some(shared);
        self.irq = Some(irq);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_gpio_irq: remove, deregistering /dev/rust_gpio_irq\n");
        self.misc.take();
        self.irq.take();
        self.shared.take();
    }
}

/// Module state: the platform device and the driver registration.
/// Kept alive for the module's lifetime; dropping them unregisters
/// the device (and so runs `remove`) and the driver.
struct RustGpioIrq {
    _dev: platform::Device,
    _reg: platform::Registration<RustGpioIrqDriver>,
}

impl kernel::Module for RustGpioIrq {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_gpio_irq: registering platform driver\n");
        let drv = RustGpioIrqDriver {
            misc: None,
            irq: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustGpioIrqDriver>::register(drv, module.as_ptr())?;
        pr_info!("rust_gpio_irq: creating platform device\n");
        let dev = platform::Device::create(c_str!("rust_gpio_irq"), -1)?;
        Ok(Self { _dev: dev, _reg: reg })
    }
}
