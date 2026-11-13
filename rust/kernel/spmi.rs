// SPDX-License-Identifier: GPL-2.0

//! SPMI bus driver abstraction (Step 11, option 1).
//!
//! SPMI (System Power Management Interface) is the bus the PMIC
//! hangs off: the ADC, the RTC, the GPIOs and the regulators are
//! all SPMI slaves of the PMIC arbiter (`qcom,spmi@800f000`).
//! A driver that talks to one of them binds to the slave device
//! the SPMI core enumerates from the device tree, the same way a
//! platform driver binds to a platform device -- except the bus
//! matches on the device-tree `compatible` string, not on a
//! device name, so the match table is an `of_device_id` array
//! (a field of `SpmiDriverInner`, pointed at by
//! `of_match_table` once the struct is on the heap).
//!
//! [`SpmiDevice`] is a transparent wrapper over
//! `*mut bindings::spmi_device`; the device is owned by the
//! SPMI core and is only borrowed for the duration of the
//! callback. [`SpmiDevice::read_ext`] / [`SpmiDevice::write_u8`]
//! are the register accessors: `spmi_ext_register_readl` (the
//! 16-bit-address multi-byte read) and `spmi_register_write`
//! (the single-byte write). The 8-bit `spmi_register_read` is
//! not wrapped: the PMIC registers this tree uses are all
//! 16-bit addresses, and the one-byte read is the same call
//! with a length of one.

use crate::{
    bindings,
    c_str,
    error::{Error, Result},
    prelude::*,
    str::CStr,
};
use core::ffi::c_int;

/// An SPMI slave device, as passed to `probe`/`remove`.
///
/// A transparent wrapper over `*mut bindings::spmi_device`; the
/// device is owned by the SPMI core and is only borrowed for the
/// duration of the callback.
#[repr(transparent)]
pub struct SpmiDevice {
    ptr: *mut bindings::spmi_device,
}

// SAFETY: the pointer is only ever handed to the SPMI core
// (`spmi_ext_register_readl`, `spmi_register_write`); Rust never
// dereferences it, and the device is owned by the SPMI core for
// the duration of the callback.
unsafe impl Send for SpmiDevice {}
unsafe impl Sync for SpmiDevice {}

impl SpmiDevice {
    /// Read `buf.len()` bytes starting at the 16-bit SPMI
    /// register `addr` (`spmi_ext_register_readl`).
    pub fn read_ext(&self, addr: u16, buf: &mut [u8]) -> Result<()> {
        // SAFETY: `spmi_ext_register_readl` is the documented
        // multi-byte read; `buf` is a live Rust slice of the
        // length passed. It returns a negative errno on failure.
        let ret = unsafe {
            bindings::spmi_ext_register_readl(
                self.ptr,
                addr,
                buf.as_mut_ptr(),
                buf.len(),
            )
        };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    /// Read one byte from the 16-bit SPMI register `addr`.
    pub fn read_u8(&self, addr: u16) -> Result<u8> {
        let mut buf = [0u8; 1];
        self.read_ext(addr, &mut buf)?;
        Ok(buf[0])
    }

    /// Write one byte to the 8-bit SPMI register `addr`
    /// (`spmi_register_write`). Only for the few PMIC registers
    /// that live below 0x100; everything else uses the 16-bit
    /// form through the regmap instead.
    pub fn write_u8(&self, addr: u8, data: u8) -> Result<()> {
        // SAFETY: `spmi_register_write` is the documented
        // single-byte write; it returns a negative errno on
        // failure.
        let ret = unsafe { bindings::spmi_register_write(self.ptr, addr, data) };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    /// Write `buf.len()` bytes starting at the 16-bit SPMI
    /// register `addr` (`spmi_ext_register_writel`).
    pub fn write_ext(&self, addr: u16, buf: &[u8]) -> Result<()> {
        // SAFETY: `spmi_ext_register_writel` is the documented
        // multi-byte write; `buf` is a live Rust slice of the
        // length passed. It returns a negative errno on failure.
        let ret = unsafe {
            bindings::spmi_ext_register_writel(
                self.ptr,
                addr,
                buf.as_ptr(),
                buf.len(),
            )
        };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    /// Write one byte to the 16-bit SPMI register `addr`.
    pub fn write_u16(&self, addr: u16, data: u8) -> Result<()> {
        self.write_ext(addr, &[data])
    }

    /// The underlying device pointer, for passing to C functions
    /// that take one (for example `dev_get_regmap` on the
    /// parent, which yields the PMIC regmap).
    pub fn as_ptr(&self) -> *mut bindings::spmi_device {
        self.ptr
    }

    /// The parent `struct device` (the PMIC), for
    /// `dev_get_regmap`. The SPMI slaves in this tree are all
    /// children of the PMIC, whose regmap is the one their C
    /// drivers use.
    pub fn parent_device(&self) -> *mut bindings::device {
        // SAFETY: `dev` is the first field of `spmi_device`,
        // and `parent` is a field of `struct device`; both are
        // live for the duration of the callback.
        unsafe { (*self.ptr).dev.parent }
    }

    /// Open a direct handle to an SPMI slave by its slave ID,
    /// without binding a driver to it.
    ///
    /// This is for slaves that already have a C driver (the
    /// PMIC RTC, the PMIC ADC): the SPMI bus only binds one
    /// driver per device, so a Rust SPMI driver can never
    /// probe them. Instead this finds the SPMI controller
    /// (a platform device, `qcom,spmi-pmic-arb`), takes the
    /// controller from its driver data, and allocates a
    /// synthetic `spmi_device` for the slave ID. The device
    /// is never added to the bus, so it cannot conflict with
    /// the C driver; it is only a handle for
    /// `spmi_ext_register_readl` / `spmi_ext_register_writel`,
    /// which use just `ctrl` and `usid`.
    ///
    /// The allocated device is leaked deliberately: the
    /// caller holds it for the driver's lifetime, and the
    /// driver is built in and never unloaded.
    pub fn for_slave(sid: u8) -> Result<Self> {
        // Find the SPMI controller's device-tree node.
        // SAFETY: `of_find_compatible_node` is the documented
        // lookup; NULL `from` starts at the root, NULL `type`
        // matches any type. NULL means no such node.
        let node = unsafe {
            bindings::of_find_compatible_node(
                core::ptr::null_mut(),
                core::ptr::null(),
                c_str!("qcom,spmi-pmic-arb").as_ptr(),
            )
        };
        if node.is_null() {
            return Err(ENODEV);
        }
        // Find the controller's platform device.
        // SAFETY: `of_find_device_by_node` is the documented
        // lookup for a platform device by its node. NULL means
        // no such device.
        let pdev = unsafe { bindings::of_find_device_by_node(node) };
        if pdev.is_null() {
            return Err(ENODEV);
        }
        // The arbiter driver stores the `spmi_controller` as
        // the platform device's driver data
        // (`platform_set_drvdata(pdev, ctrl)`).
        // SAFETY: `driver_data` is the field `dev_get_drvdata`
        // reads; the arbiter driver set it to the controller.
        let ctrl = unsafe { (*pdev).dev.driver_data as *mut bindings::spmi_controller };
        if ctrl.is_null() {
            return Err(ENODEV);
        }
        // Allocate a synthetic slave device for this slave ID.
        // SAFETY: `spmi_device_alloc` is the documented
        // allocator; NULL means the allocation failed. The
        // device is never added to the bus.
        let sdev = unsafe { bindings::spmi_device_alloc(ctrl) };
        if sdev.is_null() {
            return Err(ENOMEM);
        }
        // SAFETY: `usid` is a plain field of `spmi_device`;
        // the PMIC is slave 0.
        unsafe { (*sdev).usid = sid };
        Ok(Self { ptr: sdev })
    }
}

/// The trait a Rust SPMI driver implements.
///
/// `COMPATIBLE` is the device-tree `compatible` string the bus
/// matches on (for example `qcom,pm8941-rtc` for the PMIC RTC).
/// `probe` receives the bound slave device; it returns an error
/// (and the core unbinds) when the device cannot be used.
pub trait SpmiDriver: Send + Sync + Sized {
    /// The driver name, used for the `device_driver` and for
    /// logging. Must be unique among SPMI drivers.
    const NAME: &'static CStr;
    /// The device-tree `compatible` string to bind to.
    const COMPATIBLE: &'static CStr;
    /// Called when the SPMI core binds this driver to a slave.
    fn probe(&mut self, sdev: &SpmiDevice) -> Result<()>;
    /// Called when the driver is unbound. Optional.
    fn remove(&mut self, _sdev: &SpmiDevice) {}
}

/// The `spmi_driver`, the match table and the driver state, in
/// C layout.
///
/// `#[repr(C)]` with `sdrv` first: the SPMI core only ever sees
/// `&sdrv` (handed to `__spmi_driver_register`), and the probe
/// callback recovers this struct from `sdev->dev.driver` with
/// `container_of!` on `sdrv.driver`, so `sdrv` must be at
/// offset 0. A plain Rust struct once reordered these fields
/// and crashed the kernel (see `miscdevice.rs`); the layout is
/// pinned here for the same reason.
///
/// The match table lives in this struct (not in a separate
/// allocation) so the `of_match_table` pointer is stable: the
/// struct is heap-allocated by `Box::new`, and the pointer is
/// set after the allocation, so it points at the final heap
/// location and never dangles.
#[repr(C)]
struct SpmiDriverInner<T: SpmiDriver> {
    sdrv: bindings::spmi_driver,
    match_table: [bindings::of_device_id; 2],
    drv: T,
}

impl<T: SpmiDriver> SpmiDriverInner<T> {
    fn sdrv_ptr(&mut self) -> *mut bindings::spmi_driver {
        &mut self.sdrv as *mut bindings::spmi_driver
    }
}

// SAFETY: the raw pointers inside `sdrv` (`device_driver.name`,
// `owner`, `bus`, `of_match_table`, ...) are never dereferenced
// by Rust code; the SPMI core owns them under its own locking,
// and Rust never writes `sdrv` after registration. The match
// table is only read by the SPMI core (for matching). Sharing
// the struct across threads, and moving it across threads, is
// therefore sound as long as the driver state (`T`) is itself
// safe to share and move (the `SpmiDriver: Send + Sync` bound).
unsafe impl<T: SpmiDriver> Send for SpmiDriverInner<T> {}
unsafe impl<T: SpmiDriver> Sync for SpmiDriverInner<T> {}

impl<T: SpmiDriver> SpmiDriverInner<T> {
    /// The probe trampoline: recover `Self` from the driver
    /// pointer the SPMI core stashed in the device, then call
    /// the Rust `probe`. The `spmi_driver` is at offset 0 of
    /// this struct, and `sdev->dev.driver` points at its
    /// `device_driver`, so `container_of!` on the driver
    /// yields this struct.
    extern "C" fn probe_trampoline(sdev: *mut bindings::spmi_device) -> c_int {
        // SAFETY: `sdev` is a live SPMI device handed to us by
        // the SPMI core; `sdev->dev.driver` is the
        // `device_driver` of the driver that bound to it, which
        // is the `driver` field of our `spmi_driver` at offset
        // 0 of `SpmiDriverInner`. `container_of!` returns a
        // `*const`; it is cast to `*mut` (as in platform.rs)
        // because the driver state is mutated by `probe`.
        let inner = unsafe {
            crate::container_of!(
                (*sdev).dev.driver,
                SpmiDriverInner<T>,
                sdrv.driver
            ) as *mut SpmiDriverInner<T>
        };
        // SAFETY: `inner` is the live driver state; `sdev` is
        // the device being probed.
        let sdev = SpmiDevice { ptr: sdev };
        let inner = unsafe { &mut *inner };
        match inner.drv.probe(&sdev) {
            Ok(()) => 0,
            Err(e) => e.to_errno(),
        }
    }

    /// The remove trampoline: same recovery as `probe`, then
    /// call the Rust `remove`.
    extern "C" fn remove_trampoline(sdev: *mut bindings::spmi_device) {
        // SAFETY: same as `probe_trampoline`.
        let inner = unsafe {
            crate::container_of!(
                (*sdev).dev.driver,
                SpmiDriverInner<T>,
                sdrv.driver
            ) as *mut SpmiDriverInner<T>
        };
        let sdev = SpmiDevice { ptr: sdev };
        let inner = unsafe { &mut *inner };
        inner.drv.remove(&sdev);
    }
}

/// A registered SPMI driver. Created by [`Registration::register`];
/// dropping it unregisters the driver (which runs `remove` on
/// every bound device).
pub struct Registration<T: SpmiDriver> {
    inner: Box<SpmiDriverInner<T>>,
}

impl<T: SpmiDriver> Registration<T> {
    /// Register an SPMI driver (`__spmi_driver_register`).
    /// The match table is filled from `T::COMPATIBLE` and the
    /// `of_match_table` pointer is set after the struct is on
    /// the heap, so it points at the final location.
    pub fn register(drv: T, owner: *mut bindings::module) -> Result<Self> {
        let mut inner = Box::new(
            SpmiDriverInner {
                sdrv: bindings::spmi_driver::default(),
                match_table: [
                    bindings::of_device_id::default(),
                    bindings::of_device_id::default(),
                ],
                drv,
            },
            GFP_KERNEL,
        )
        .map_err(|_| ENOMEM)?;
        // Fill the match table's `compatible` (an inline
        // `[c_char; 128]`) with the driver's compatible string;
        // the rest stays zero (NUL-terminated). The second
        // entry stays all-zero, which is the table terminator.
        let compat = T::COMPATIBLE.as_bytes();
        for (i, &b) in compat.iter().enumerate() {
            if i >= 127 {
                break;
            }
            inner.match_table[0].compatible[i] = b as core::ffi::c_char;
        }
        inner.sdrv.driver.name = T::NAME.as_ptr();
        inner.sdrv.driver.owner = owner;
        // The pointer is set after the Box allocation, so it
        // points at the match table in the final heap location.
        // The Box is owned by `Registration` and never moves,
        // so the pointer stays valid for the driver's lifetime.
        inner.sdrv.driver.of_match_table = inner.match_table.as_ptr();
        inner.sdrv.probe = Some(SpmiDriverInner::<T>::probe_trampoline);
        inner.sdrv.remove = Some(SpmiDriverInner::<T>::remove_trampoline);
        // SAFETY: `sdrv` is a fully-initialized `spmi_driver`;
        // the core takes the pointer until `driver_unregister`.
        // `__spmi_driver_register` is the documented
        // registration (the `spmi_driver_register` macro itself
        // cannot be called from Rust, so the `THIS_MODULE`
        // owner is passed explicitly).
        let ret = unsafe {
            bindings::__spmi_driver_register(inner.sdrv_ptr(), owner)
        };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(Self { inner })
    }
}

impl<T: SpmiDriver> Drop for Registration<T> {
    fn drop(&mut self) {
        // SAFETY: registered by `register()`; the core releases
        // the pointer on unregister. `spmi_driver_unregister`
        // is a static inline that does exactly this
        // `driver_unregister` on the `device_driver`; the
        // driver core runs `remove` first.
        unsafe { bindings::driver_unregister(&mut self.inner.sdrv.driver) }
    }
}
