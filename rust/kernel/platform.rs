// SPDX-License-Identifier: GPL-2.0

//! Platform driver abstraction.
//!
//! A platform driver binds to a platform device through the Linux
//! driver model. The bus matches driver and device by OF/ACPI/id
//! table first, and falls back to comparing names
//! (`platform_match`: `strcmp(pdev->name, drv->name) == 0`), so a
//! driver can be tested without any device-tree change by creating
//! its own device with [`Device::create`].
//!
//! Only Rust-side code is involved here: the C platform core
//! (`drivers/base/platform.c`) is untouched. The Rust side fills in
//! a `struct platform_driver` and hands its address to
//! `__platform_driver_register` (the `platform_driver_register`
//! macro itself cannot be called from Rust, so the `THIS_MODULE`
//! owner is passed explicitly, the same way the `file_operations`
//! owner is passed in `miscdevice`).

use crate::{
    bindings,
    error::{code::*, from_err_ptr, Error, Result},
    prelude::*,
    str::CStr,
};
use core::ffi::c_int;

/// A platform device, as passed to `probe`/`remove`.
///
/// A transparent wrapper over `*mut bindings::platform_device`; the
/// device is owned by the platform core and is only borrowed for the
/// duration of the callback.
#[repr(transparent)]
pub struct PlatformDevice {
    ptr: *mut bindings::platform_device,
}

impl PlatformDevice {
    /// Wrap a raw device pointer.
    ///
    /// # Safety
    /// `ptr` must point to a live `struct platform_device` (the
    /// platform core guarantees this inside `probe`/`remove`).
    pub unsafe fn from_ptr(ptr: *mut bindings::platform_device) -> Self {
        Self { ptr }
    }

    /// Device name (`pdev->name`). Devices created by the platform
    /// core always have a name.
    pub fn name(&self) -> &CStr {
        // SAFETY: the device is live for the duration of the callback
        // and always carries a name.
        unsafe { CStr::from_char_ptr((*self.ptr).name) }
    }

    /// Device id (`pdev->id`; `-1` for the single-instance devices
    /// created by [`Device::create`]).
    pub fn id(&self) -> i32 {
        // SAFETY: same as [`PlatformDevice::name`].
        unsafe { (*self.ptr).id }
    }

    /// The raw `struct platform_device` pointer, for C APIs that
    /// need the device itself (for example the bus-scaling client
    /// registration, which reads the device's `of_node`).
    pub fn as_ptr(&self) -> *mut bindings::platform_device {
        self.ptr
    }

    /// The `index`-th `IORESOURCE_MEM` resource as `(start, size)`.
    ///
    /// Returns `None` when the device has no such resource. The
    /// resource is owned by the device (or by the firmware that
    /// described it); only its address range is copied out.
    pub fn mem_resource(&self, index: u32) -> Option<(u64, usize)> {
        // SAFETY: the device is live for the duration of the
        // callback; `platform_get_resource` returns a pointer to
        // the device's own resource array, or NULL.
        let res = unsafe {
            bindings::platform_get_resource(self.ptr, bindings::IORESOURCE_MEM, index)
        };
        if res.is_null() {
            return None;
        }
        // SAFETY: the resource is live (owned by the device) and
        // `start`/`end` are plain integers.
        let (start, end) = unsafe { ((*res).start, (*res).end) };
        if end < start {
            return None;
        }
        Some((start, (end - start + 1) as usize))
    }
}

/// A platform driver implementation.
///
/// `probe` runs when the driver model binds a matching device;
/// `remove` runs on unbind. Both take `&mut self` so the driver can
/// own per-driver state (for example, a misc device registered in
/// `probe` and deregistered in `remove`).
///
/// The probe/remove callbacks are the sole accessor of the driver
/// state while the driver is registered, and this abstraction is
/// used with a single device per driver, so `&mut` is sound here. A
/// driver binding several devices at once must synchronize its own
/// state (for example with a `Mutex`) instead of relying on `&mut`.
pub trait PlatformDriver: Send + Sync + Sized {
    /// Driver name, used for matching (`driver.name` vs `pdev->name`).
    const NAME: &'static CStr;

    /// Called when a matching device is bound.
    fn probe(&mut self, pdev: &PlatformDevice) -> Result<()>;

    /// Called when the device is unbound.
    fn remove(&mut self, pdev: &PlatformDevice);
}

/// The C `platform_driver` plus the Rust driver state.
///
/// `#[repr(C)]` with `pdrv` first: the platform core only ever sees
/// `&pdrv` (handed to `__platform_driver_register`), and the probe
/// callback recovers this struct from `pdev->dev.driver` with
/// `container_of!` on `pdrv.driver`, so `pdrv` must be at offset 0.
/// A plain Rust struct once reordered these fields and crashed the
/// kernel (see `miscdevice.rs`); the layout is pinned here for the
/// same reason.
#[repr(C)]
struct PlatformDriverInner<T: PlatformDriver> {
    pdrv: bindings::platform_driver,
    drv: T,
}

impl<T: PlatformDriver> PlatformDriverInner<T> {
    fn pdrv_ptr(&mut self) -> *mut bindings::platform_driver {
        &mut self.pdrv as *mut _
    }
}

// SAFETY: the raw pointers inside `pdrv` (`device_driver.name`,
// `owner`, `bus`, match tables, ...) are never dereferenced by Rust
// code; the platform core owns them under its own locking, and Rust
// never writes `pdrv` after registration. Sharing the struct across
// threads, and moving it across threads, is therefore sound as long
// as the driver state (`T`) is itself safe to share and move (the
// `PlatformDriver: Send + Sync` bound).
unsafe impl<T: PlatformDriver> Send for PlatformDriverInner<T> {}
unsafe impl<T: PlatformDriver> Sync for PlatformDriverInner<T> {}

/// A registered platform driver. Unregisters the driver on drop.
pub struct Registration<T: PlatformDriver> {
    inner: Box<PlatformDriverInner<T>>,
}

impl<T: PlatformDriver> Registration<T> {
    /// Register the driver. `owner` is the registering module's
    /// `THIS_MODULE` (from `ThisModule::as_ptr()`).
    pub fn register(drv: T, owner: *mut bindings::module) -> Result<Self> {
        let mut inner = Box::new(
            PlatformDriverInner {
                pdrv: bindings::platform_driver::default(),
                drv,
            },
            GFP_KERNEL,
        )
        .map_err(|_| ENOMEM)?;
        inner.pdrv.probe = Some(probe_callback::<T>);
        inner.pdrv.remove = Some(remove_callback::<T>);
        inner.pdrv.driver.name = T::NAME.as_ptr();
        inner.pdrv.driver.owner = owner;
        // SAFETY: `pdrv` is a fully-initialized `platform_driver`;
        // the core takes the pointer until `platform_driver_unregister`.
        let ret = unsafe { bindings::__platform_driver_register(inner.pdrv_ptr(), owner) };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(Self { inner })
    }
}

impl<T: PlatformDriver> Drop for Registration<T> {
    fn drop(&mut self) {
        // SAFETY: registered by `register()`; the core releases the
        // pointer on unregister.
        unsafe { bindings::platform_driver_unregister(self.inner.pdrv_ptr()) }
    }
}

/// Recover the driver state from a bound device.
///
/// # Safety
/// `pdev` must be a live device bound to this driver, so
/// `pdev->dev.driver` is our `device_driver` (embedded in `pdrv`,
/// field 0 of the inner struct). Returns NULL when the device is
/// not (yet) bound.
unsafe fn inner_from_pdev<T: PlatformDriver>(
    pdev: *mut bindings::platform_device,
) -> *mut PlatformDriverInner<T> {
    // SAFETY: probe/remove are only called on a live device.
    let drv = unsafe { (*pdev).dev.driver };
    if drv.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `drv` is the `driver` field of our `pdrv`
    // (`container_of!` on `platform_driver.driver`), and `pdrv` is
    // field 0 of the inner struct, so the two addresses coincide.
    let pdrv = unsafe { crate::container_of!(drv, bindings::platform_driver, driver) };
    pdrv as *mut PlatformDriverInner<T>
}

unsafe extern "C" fn probe_callback<T: PlatformDriver>(
    pdev: *mut bindings::platform_device,
) -> c_int {
    let inner = unsafe { inner_from_pdev::<T>(pdev) };
    if inner.is_null() {
        return ENODEV.to_errno();
    }
    // SAFETY: `inner` is the live driver state; this callback is its
    // sole accessor while the driver is registered.
    let inner = unsafe { &mut *inner };
    let pdev = unsafe { PlatformDevice::from_ptr(pdev) };
    match inner.drv.probe(&pdev) {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}

unsafe extern "C" fn remove_callback<T: PlatformDriver>(
    pdev: *mut bindings::platform_device,
) -> c_int {
    let inner = unsafe { inner_from_pdev::<T>(pdev) };
    if !inner.is_null() {
        // SAFETY: same as `probe_callback`.
        let inner = unsafe { &mut *inner };
        let pdev = unsafe { PlatformDevice::from_ptr(pdev) };
        inner.drv.remove(&pdev);
    }
    0
}

/// A platform device created with `platform_device_alloc` + `add`.
/// Unregisters (and frees) the device on drop.
pub struct Device {
    pdev: *mut bindings::platform_device,
    /// Resources owned by this struct. The platform core keeps a
    /// pointer to this array (`pdev->resource`) for the device's
    /// lifetime, so it must outlive the device: `Drop` unregisters
    /// the device first, and only then are the fields dropped.
    resources: Vec<bindings::resource>,
}

// SAFETY: the pointer is only ever handed back to the C core
// (`platform_device_unregister` in `Drop`); Rust never dereferences
// it, and the device is owned by this struct until then. The
// resource array is only read by the core while the device is
// registered; Rust never writes it after `add`.
unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl Device {
    /// Create and register a device. The core copies `name`
    /// (`platform_device_alloc` duplicates it into the allocation),
    /// so it need not be `'static`.
    pub fn create(name: &CStr, id: i32) -> Result<Self> {
        Self::create_with_resources(name, id, &[])
    }

    /// Create and register a device with resources. The resources
    /// are copied into owned storage; the core keeps the pointer
    /// (`pdev->resource`) until the device is unregistered, so they
    /// must stay put for the device's lifetime.
    pub fn create_with_resources(
        name: &CStr,
        id: i32,
        resources: &[bindings::resource],
    ) -> Result<Self> {
        // SAFETY: `alloc`/`add` are the documented constructors;
        // `alloc` returns an `ERR_PTR` on failure.
        let pdev = from_err_ptr(unsafe {
            bindings::platform_device_alloc(name.as_ptr(), id)
        })?;
        let mut owned: Vec<bindings::resource> = Vec::new();
        for r in resources {
            owned.push(*r, GFP_KERNEL).map_err(|_| ENOMEM)?;
        }
        if !owned.is_empty() {
            // SAFETY: `pdev` is live and allocated; the Vec buffer
            // outlives the device (it is a field of `Device`,
            // dropped after `unregister` in `Drop`), so the core
            // can safely keep the pointer until then.
            unsafe {
                (*pdev).resource = owned.as_mut_ptr();
                (*pdev).num_resources = owned.len() as u32;
            }
        }
        // SAFETY: `pdev` is a live, allocated device. On failure
        // `add` leaves it allocated, so drop the reference before
        // returning the error.
        let ret = unsafe { bindings::platform_device_add(pdev) };
        if ret != 0 {
            unsafe { bindings::platform_device_put(pdev) };
            return Err(Error::from_errno(ret));
        }
        Ok(Self { pdev, resources: owned })
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: created by `create()`; unregister releases the
        // core's reference and frees the device.
        unsafe { bindings::platform_device_unregister(self.pdev) }
    }
}
