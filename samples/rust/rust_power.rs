// SPDX-License-Identifier: GPL-2.0

//! Power control driver (Step 10, option 3).
//!
//! Makes the `regulator` abstraction actionable: where `rust_hwmon`
//! only *reads* a regulator, this driver lets userspace turn one
//! on and off. It controls `pm660_l1` (the 1.2 V rail `rust_hwmon`
//! already reads) through `/dev/rust_power`:
//!
//! ```text
//! $ echo "l1 on" > /dev/rust_power
//! $ cat /dev/rust_power
//! l1_enabled=1
//! l1_uv=1200000
//! l1_mine=1
//! $ echo "l1 off" > /dev/rust_power
//! ```
//!
//! Safety comes from the regulator core's reference counting and
//! from tracking what *this* driver did: `enable` only bumps the
//! use count, and this driver only ever calls `disable` for a
//! regulator it enabled itself (`l1_mine`). It can never turn the
//! rail off while the system still holds a reference, and an
//! `off` for a rail it never turned on is a no-op, not an
//! unbalanced disable.

use core::sync::atomic::{AtomicBool, Ordering};

use kernel::{
    bindings,
    c_str,
    file::{File, FileOperations, UserSliceReader, UserSliceWriter},
    miscdevice::{Registration as MiscRegistration, DYNAMIC_MINOR},
    platform::{self, PlatformDevice, PlatformDriver},
    prelude::*,
    regulator::Regulator,
    sync::{Arc, UniqueArc},
};
use kernel::text::{copy_bytes, write_u32_dec, write_line};

module! {
    type: RustPower,
    name: "rust_power",
    author: "MayaKernel",
    description: "Rust power control (pm660_l1 enable/disable)",
    license: "GPL",
}

/// pm660 L1 regulator name.
const PM660_L1: &CStr = c_str!("pm660_l1");

/// Shared device state: the regulator and whether this driver
/// turned it on. The flag is atomic because `read` and `write`
/// can run on different threads.
#[pin_data]
struct Shared {
    reg: Regulator,
    enabled_by_me: AtomicBool,
}

impl Shared {
    /// Render the current state: the rail's actual enable state
    /// and voltage, plus whether this driver is the one holding
    /// it on.
    fn snapshot(&self) -> [u8; 64] {
        let mut buf = [0u8; 64];
        let mut len = 0usize;
        let enabled = self.reg.is_enabled().unwrap_or(false);
        len += write_line(&mut buf[len..], b"l1_enabled=", enabled as u32);
        let uv = self.reg.voltage().unwrap_or(0);
        len += write_line(&mut buf[len..], b"l1_uv=", uv as u32);
        let mine = self.enabled_by_me.load(Ordering::Relaxed);
        len += write_line(&mut buf[len..], b"l1_mine=", mine as u32);
        buf
    }

    /// The length of the snapshot text (up to the first NUL).
    fn snapshot_len(buf: &[u8; 64]) -> usize {
        buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
    }
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct PowerOps {
    shared: Arc<Shared>,
}

impl FileOperations for PowerOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        // A live snapshot, not a stream: every read re-reads
        // the regulator. Honour the offset so `cat` sees EOF.
        let buf = self.shared.snapshot();
        let len = Shared::snapshot_len(&buf);
        let offset = offset as usize;
        if offset >= len {
            return Ok(0);
        }
        writer.write(&buf[offset..])
    }

    fn write(&self, _file: &File, reader: &mut UserSliceReader, _offset: u64) -> Result<usize> {
        // Commands are `l1 on` and `l1 off` (a trailing newline
        // is fine). Read the whole command, act on its prefix,
        // and consume it all so the shell does not retry.
        let mut cmd = [0u8; 16];
        let n = reader.read(&mut cmd)?;
        if n == 0 {
            return Ok(0);
        }
        let cmd = &cmd[..n];
        if cmd.starts_with(b"l1 on") {
            if !self.shared.enabled_by_me.load(Ordering::Relaxed) {
                self.shared.reg.enable()?;
                self.shared.enabled_by_me.store(true, Ordering::Relaxed);
                pr_info!("rust_power: l1 on (by me)\n");
            }
        } else if cmd.starts_with(b"l1 off") {
            if self.shared.enabled_by_me.load(Ordering::Relaxed) {
                self.shared.reg.disable()?;
                self.shared.enabled_by_me.store(false, Ordering::Relaxed);
                pr_info!("rust_power: l1 off (by me)\n");
            } else {
                pr_info!("rust_power: l1 off ignored (not mine)\n");
            }
        }
        Ok(n)
    }
}

/// Driver state. The misc device is registered in `probe`, so
/// `misc` and `shared` start empty; the module pointer is the
/// only field `init` fills in.
struct RustPowerDriver {
    misc: Option<MiscRegistration<PowerOps>>,
    shared: Option<Arc<Shared>>,
    owner: *mut bindings::module,
}

// SAFETY: `owner` is only ever passed to registration functions
// (which store it, never dereference it from Rust); every other
// field is `Send + Sync` on its own.
unsafe impl Send for RustPowerDriver {}
unsafe impl Sync for RustPowerDriver {}

impl PlatformDriver for RustPowerDriver {
    const NAME: &'static CStr = c_str!("rust_power");

    fn probe(&mut self, pdev: &PlatformDevice) -> Result<()> {
        pr_info!("rust_power: probe, driver bound\n");

        // Get pm660 L1 (proven in Steps 7-9). Do not enable it
        // here: it stays off until userspace asks for it.
        let reg = Regulator::get(PM660_L1)?;
        let uv = reg.voltage()?;
        let enabled = reg.is_enabled()?;
        pr_info!("rust_power: pm660_l1 = {} uV, enabled = {}\n", uv, enabled);

        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared {
                reg,
                enabled_by_me: AtomicBool::new(false),
            }))?
            .into();
        let misc = MiscRegistration::<PowerOps>::new(
            c_str!("rust_power"),
            DYNAMIC_MINOR,
            shared.clone(),
            self.owner,
        )?;
        pr_info!("rust_power: registered /dev/rust_power\n");

        self.shared = Some(shared);
        self.misc = Some(misc);
        Ok(())
    }

    fn remove(&mut self, _pdev: &PlatformDevice) {
        pr_info!("rust_power: remove, deregistering /dev/rust_power\n");
        // If this driver turned the rail on, turn it back off
        // so it does not leak a reference across a remove.
        if let Some(shared) = self.shared.as_ref() {
            if shared.enabled_by_me.load(Ordering::Relaxed) {
                let _ = shared.reg.disable();
                shared.enabled_by_me.store(false, Ordering::Relaxed);
            }
        }
        self.misc.take();
        self.shared.take();
    }
}

/// Module state: the platform device and the driver registration.
/// Kept alive for the module's lifetime; dropping them unregisters
/// the device (and so runs `remove`) and the driver.
struct RustPower {
    _dev: platform::Device,
    _reg: platform::Registration<RustPowerDriver>,
}

impl kernel::Module for RustPower {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_power: registering platform driver\n");
        let drv = RustPowerDriver {
            misc: None,
            shared: None,
            owner: module.as_ptr(),
        };
        let reg = platform::Registration::<RustPowerDriver>::register(drv, module.as_ptr())?;
        pr_info!("rust_power: creating platform device\n");
        let dev = platform::Device::create(c_str!("rust_power"), -1)?;
        Ok(Self { _dev: dev, _reg: reg })
    }
}
