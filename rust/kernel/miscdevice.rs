// SPDX-License-Identifier: GPL-2.0

//! Misc devices.
//!
//! C header: [`include/linux/miscdevice.h`](srctree/include/linux/miscdevice.h)
//!
//! A misc device is a simple character device with a dynamically allocated
//! minor number, registered with [`misc_register`](bindings::misc_register).
//! The device shows up as `/dev/<name>` and serves the [`FileOperations`]
//! of the driver that registered it.
//!
//! [`FileOperations`]: crate::file::FileOperations

use crate::{
    alloc::{box_ext::BoxExt, flags::GFP_KERNEL},
    bindings,
    error::{code::*, Error, Result},
    file::{build_fops, FileOperations},
    str::CStr,
    types::Opaque,
};
use alloc::boxed::Box;

/// A misc device.
///
/// The struct is heap-allocated and never moved after registration:
/// [`misc_register`](bindings::misc_register) stores the address of its
/// `inner` field, and [`Registration`] owns the allocation for the whole
/// lifetime of the registration. The address is stable because a `Box`
/// allocation never moves its contents, and [`Registration`] never moves
/// the value out of the box.
///
/// The `fops` field is a `file_operations` vtable built by [`build_fops`]
/// for the driver's [`FileOperations`] implementation. It is stored inside
/// this struct (rather than in a `static`) because it embeds `owner`, the
/// registering module's `THIS_MODULE`, which is only known at registration
/// time. `inner.fops` points at it; the pointer is fixed up in
/// [`Registration::new`] before registration, while the struct is already
/// heap-allocated, and never changes afterwards.
/// The `inner` field MUST stay first: the C misc core stores the address of
/// `inner` in `file->private_data`, and `open_callback` recovers the
/// `MiscDevice` from it by casting that address to `*const MiscDevice`.
/// That cast is only correct if `inner` is at offset 0, so the layout is
/// pinned with `#[repr(C)]` (a plain Rust struct has no guaranteed field
/// order). Without it, the compiler once put `data` first and `open` read
/// the device's minor number (97) as an `Arc` pointer, crashing the kernel
/// with a NULL dereference at 0x61 in `refcount_inc`.
#[repr(C)]
pub struct MiscDevice<T: FileOperations> {
    inner: Opaque<bindings::miscdevice>,
    fops: bindings::file_operations,
    data: T::OpenData,
}

impl<T: FileOperations> MiscDevice<T> {
    /// Returns the device context shared by all files opened on the device.
    pub(crate) fn open_data(&self) -> &T::OpenData {
        &self.data
    }
}

// SAFETY: `MiscDevice` is only ever accessed through shared references (from
// `open_callback`, which only reads `data`) or through the single owning
// `Registration` (which registers/deregisters it under the C misc core's own
// locking, `misc_mtx`). The C pointers inside (`inner`, `fops`) are never
// dereferenced by Rust code; the C core owns the objects they point at, and
// Rust never writes to `inner` after registration. Sharing the device across
// threads is therefore sound as long as the device context (`T::OpenData`)
// is itself safe to share, and moving it across threads is sound as long as
// the context is safe to move.
unsafe impl<T: FileOperations> Send for MiscDevice<T> where T::OpenData: Send {}
unsafe impl<T: FileOperations> Sync for MiscDevice<T> where T::OpenData: Sync {}

/// Minor number that lets the kernel pick a free minor for the device.
///
/// Equivalent to `MISC_DYNAMIC_MINOR` in `include/linux/miscdevice.h`.
pub const DYNAMIC_MINOR: i32 = bindings::MISC_DYNAMIC_MINOR as i32;

/// A registered misc device.
///
/// Deregisters the device on drop, which also frees the [`MiscDevice`].
pub struct Registration<T: FileOperations> {
    mdev: Box<MiscDevice<T>>,
}

impl<T: FileOperations> Registration<T> {
    /// Registers a new misc device with the given `name`, `minor` and `data`.
    ///
    /// `name` must live for the whole registration (use a `&'static CStr`,
    /// e.g. [`c_str!`]). `minor` is usually
    /// [`MISC_DYNAMIC_MINOR`](bindings::MISC_DYNAMIC_MINOR), which lets the
    /// kernel pick a free minor number. `owner` must be the registering
    /// module's `THIS_MODULE` (see [`crate::ThisModule::as_ptr`]) so the
    /// module cannot be unloaded while a file is open.
    ///
    /// [`c_str!`]: crate::c_str
    pub fn new(
        name: &'static CStr,
        minor: i32,
        data: T::OpenData,
        owner: *mut bindings::module,
    ) -> Result<Self> {
        let mdev = Box::new(
            MiscDevice {
                inner: Opaque::new(bindings::miscdevice::default()),
                fops: build_fops::<T>(owner),
                data,
            },
            GFP_KERNEL,
        )
        .map_err(|_| ENOMEM)?;

        // SAFETY: `mdev` is heap-allocated, so its address is stable. We fix
        // up the self-referential `inner.fops` pointer before registration,
        // and the value is never moved afterwards (`Registration` owns the
        // `Box` and never moves the value out of it). `name` is `&'static`,
        // so `inner.name` stays valid for the whole registration.
        unsafe {
            let inner = mdev.inner.get();
            (*inner).minor = minor;
            (*inner).name = name.as_ptr();
            (*inner).fops = &mdev.fops;
            // `misc_devnode` uses this for the /dev node mode; 0666 matches
            // the usual misc device default.
            (*inner).mode = 0o666;
            let ret = bindings::misc_register(inner);
            if ret != 0 {
                return Err(Error::from_errno(ret));
            }
        }

        Ok(Self { mdev })
    }
}

impl<T: FileOperations> Drop for Registration<T> {
    fn drop(&mut self) {
        // SAFETY: `mdev` is still registered (this `Registration` is its
        // only owner) and its address has not moved since registration.
        unsafe { bindings::misc_deregister(self.mdev.inner.get()) }
    }
}
