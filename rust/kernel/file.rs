// SPDX-License-Identifier: GPL-2.0

//! File operations.
//!
//! C headers: [`include/linux/fs.h`](srctree/include/linux/fs.h),
//! [`include/linux/miscdevice.h`](srctree/include/linux/miscdevice.h)
//!
//! This module provides the [`FileOperations`] trait, which Rust drivers
//! implement to serve `open` / `read` / `write` / `ioctl` / `release` on a
//! file, plus the [`File`] wrapper and the user-memory accessors
//! [`UserSliceReader`] / [`UserSliceWriter`].
//!
//! The `file_operations` vtable itself is built by [`build_fops`]. All
//! `unsafe` code that talks to the VFS lives in this file: the trampolines
//! below translate between the C callbacks and the trait methods, so code
//! that uses [`FileOperations`] only writes safe Rust.
//!
//! # Lifetime of the per-file data
//!
//! [`open_callback`] creates the per-file data with [`FileOperations::open`]
//! and stores it (boxed) in `file->private_data`. [`read_callback`],
//! [`write_callback`] and [`ioctl_callback`] borrow it for the duration of
//! the call. [`release_callback`] takes it back out of `private_data` and
//! drops it. [`FileOperations::open`] is only ever called from
//! `misc_open`, which sets `file->private_data` to the [`MiscDevice`]
//! first, so `open_callback` can find the device context there.
//!
//! [`MiscDevice`]: crate::miscdevice::MiscDevice

use crate::{
    alloc::flags::GFP_KERNEL,
    bindings,
    error::{code::*, Result},
    init::{InPlaceInit, PinInit},
};
use alloc::boxed::Box;
use core::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use core::pin::Pin;

/// A reader for a userspace slice (used to implement `write`).
///
/// Reads bytes *from* userspace memory into a kernel buffer.
pub struct UserSliceReader {
    ptr: *mut c_void,
    len: usize,
}

impl UserSliceReader {
    /// Creates a reader over `len` bytes of userspace memory at `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must point to `len` bytes of userspace memory that the current
    /// process may read, for the lifetime of the reader.
    pub unsafe fn new(ptr: *mut c_void, len: usize) -> Self {
        Self { ptr, len }
    }

    /// Returns the number of bytes left to read.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns whether there is nothing left to read.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Copies up to `out.len()` bytes from userspace into `out`.
    ///
    /// Returns the number of bytes actually copied.
    pub fn read(&mut self, out: &mut [u8]) -> Result<usize> {
        let len = core::cmp::min(out.len(), self.len);
        if len == 0 {
            return Ok(0);
        }
        // SAFETY: `ptr` points to at least `len` readable userspace bytes
        // (see `new`), and `out` is a valid kernel buffer of `len` bytes.
        let ret = unsafe {
            bindings::copy_from_user(out.as_mut_ptr().cast(), self.ptr, len as c_ulong)
        };
        if ret != 0 {
            return Err(EFAULT);
        }
        // SAFETY: advancing `ptr` by `len` stays within the original slice.
        self.ptr = unsafe { self.ptr.add(len) };
        self.len -= len;
        Ok(len)
    }

    /// Copies exactly `out.len()` bytes from userspace into `out`.
    pub fn read_all(&mut self, out: &mut [u8]) -> Result<()> {
        if out.len() > self.len {
            return Err(EINVAL);
        }
        let n = self.read(out)?;
        if n != out.len() {
            return Err(EFAULT);
        }
        Ok(())
    }
}

/// A writer for a userspace slice (used to implement `read`).
///
/// Writes bytes *to* userspace memory from a kernel buffer.
pub struct UserSliceWriter {
    ptr: *mut c_void,
    len: usize,
}

impl UserSliceWriter {
    /// Creates a writer over `len` bytes of userspace memory at `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must point to `len` bytes of userspace memory that the current
    /// process may write, for the lifetime of the writer.
    pub unsafe fn new(ptr: *mut c_void, len: usize) -> Self {
        Self { ptr, len }
    }

    /// Returns the number of bytes left to write.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns whether there is nothing left to write.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Copies up to `data.len()` bytes to userspace.
    ///
    /// Returns the number of bytes actually copied.
    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        let len = core::cmp::min(data.len(), self.len);
        if len == 0 {
            return Ok(0);
        }
        // SAFETY: `ptr` points to at least `len` writable userspace bytes
        // (see `new`), and `data` is a valid kernel buffer of `len` bytes.
        let ret = unsafe {
            bindings::copy_to_user(self.ptr, data.as_ptr().cast(), len as c_ulong)
        };
        if ret != 0 {
            return Err(EFAULT);
        }
        // SAFETY: advancing `ptr` by `len` stays within the original slice.
        self.ptr = unsafe { self.ptr.add(len) };
        self.len -= len;
        Ok(len)
    }

    /// Copies all of `data` to userspace.
    pub fn write_all(&mut self, data: &[u8]) -> Result<()> {
        if data.len() > self.len {
            return Err(EINVAL);
        }
        let n = self.write(data)?;
        if n != data.len() {
            return Err(EFAULT);
        }
        Ok(())
    }
}

/// A wrapper for `struct file`.
#[repr(transparent)]
pub struct File {
    ptr: *mut bindings::file,
}

impl File {
    /// Creates a `File` reference from a raw `struct file` pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must be a valid `struct file` for the lifetime of the
    /// returned reference.
    pub(crate) unsafe fn from_ptr<'a>(ptr: *mut bindings::file) -> &'a Self {
        // SAFETY: `File` is a transparent wrapper around the pointer.
        unsafe { &*(ptr as *const Self) }
    }

    /// Returns the file position (`f_pos`).
    pub fn pos(&self) -> u64 {
        // SAFETY: `ptr` is valid (see `from_ptr`).
        unsafe { (*self.ptr).f_pos as u64 }
    }

    /// Sets the file position (`f_pos`).
    pub fn set_pos(&self, pos: u64) {
        // SAFETY: `ptr` is valid (see `from_ptr`).
        unsafe { (*self.ptr).f_pos = pos as bindings::loff_t }
    }
}

/// File operations implemented by a Rust driver.
///
/// The per-file data is the type that implements this trait: [`open`] creates
/// it, [`read`] / [`write`] / [`ioctl`] borrow it, and [`release`] gets the
/// last mutable access to it before it is dropped.
///
/// [`open`]: FileOperations::open
/// [`read`]: FileOperations::read
/// [`write`]: FileOperations::write
/// [`ioctl`]: FileOperations::ioctl
/// [`release`]: FileOperations::release
pub trait FileOperations: Send + Sync + Sized {
    /// Data shared by all files opened on the device (the device context).
    ///
    /// [`open`] receives a reference to it. Use `()` if the driver needs no
    /// shared state.
    ///
    /// [`open`]: FileOperations::open
    type OpenData;

    /// Creates the per-file data for a newly opened file.
    ///
    /// The returned value is pin-initialised into a `Box` owned by the file,
    /// so types containing a [`Mutex`] (or anything else that must not move
    /// after initialisation) are fine here.
    ///
    /// Note: the initializer is infallible (`E = Infallible`), which covers
    /// the usual case (e.g. [`new_mutex!`]). A driver that needs fallible
    /// construction must allocate its fallible state before returning the
    /// initializer, or the trait will need a fallible error type.
    ///
    /// [`Mutex`]: crate::sync::Mutex
    /// [`new_mutex!`]: crate::sync::new_mutex
    fn open(context: &Self::OpenData, file: &File) -> Result<impl PinInit<Self>>;

    /// Reads from the file into userspace.
    ///
    /// Returns the number of bytes written to userspace, or an error.
    /// Returning `Ok(0)` signals end-of-file.
    fn read(&self, _file: &File, _writer: &mut UserSliceWriter, _offset: u64) -> Result<usize> {
        Err(EINVAL)
    }

    /// Writes data from userspace into the file.
    ///
    /// Returns the number of bytes consumed, or an error.
    fn write(&self, _file: &File, _reader: &mut UserSliceReader, _offset: u64) -> Result<usize> {
        Err(EINVAL)
    }

    /// Handles an `ioctl` on the file.
    ///
    /// `arg` is the raw `unsigned long` argument from userspace (usually a
    /// userspace pointer). Returns the value for the syscall, or an error.
    fn ioctl(&self, _file: &File, _cmd: u32, _arg: usize) -> Result<isize> {
        Err(ENOTTY)
    }

    /// Called when the last reference to the file is dropped.
    ///
    /// The per-file data is dropped right after this returns.
    fn release(&mut self, _file: &File) {}
}

/// Builds the `file_operations` vtable for `T`.
///
/// `owner` must be the registering module's `THIS_MODULE` (see
/// [`crate::ThisModule::as_ptr`]) so the module cannot be unloaded while a
/// file is open. Only the operations a [`FileOperations`] implementation can
/// provide are wired up; the rest stay `NULL`.
pub(crate) fn build_fops<T: FileOperations>(owner: *mut bindings::module) -> bindings::file_operations {
    bindings::file_operations {
        owner,
        llseek: None,
        read: Some(read_callback::<T>),
        write: Some(write_callback::<T>),
        read_iter: None,
        write_iter: None,
        iterate: None,
        iterate_shared: None,
        poll: None,
        unlocked_ioctl: Some(ioctl_callback::<T>),
        compat_ioctl: Some(ioctl_callback::<T>),
        mmap: None,
        mmap_supported_flags: 0,
        open: Some(open_callback::<T>),
        flush: None,
        release: Some(release_callback::<T>),
        fsync: None,
        fasync: None,
        lock: None,
        sendpage: None,
        get_unmapped_area: None,
        check_flags: None,
        flock: None,
        splice_write: None,
        splice_read: None,
        setlease: None,
        fallocate: None,
        show_fdinfo: None,
        copy_file_range: None,
        clone_file_range: None,
        dedupe_file_range: None,
        fadvise: None,
        ..Default::default()
    }
}

/// Returns the per-file data of `file`, or `None` if `open` never ran.
fn file_data<'a, T: FileOperations>(file: *mut bindings::file) -> Option<&'a T> {
    // SAFETY: VFS passes a valid `file`. `private_data` is either null or a
    // pointer to a live `T` created by `open_callback` (see the module docs).
    let data = unsafe { (*file).private_data } as *mut T;
    if data.is_null() {
        None
    } else {
        // SAFETY: `data` points to a live `T` owned by this file; the borrow
        // lasts only for the duration of the current VFS call, during which
        // `release` cannot run on the same file.
        Some(unsafe { &*data })
    }
}

/// `open` trampoline: creates the per-file data and stores it in `private_data`.
///
/// # Safety
///
/// Called by the VFS with a valid `inode` and `file`. For misc devices,
/// `misc_open` sets `file->private_data` to the [`MiscDevice`] before
/// calling us, which is how the device context is found.
///
/// [`MiscDevice`]: crate::miscdevice::MiscDevice
unsafe extern "C" fn open_callback<T: FileOperations>(
    _inode: *mut bindings::inode,
    file: *mut bindings::file,
) -> c_int {
    // SAFETY: `file->private_data` is the `MiscDevice<T>` set by `misc_open`.
    let mdev_ptr = unsafe { (*file).private_data } as *mut crate::miscdevice::MiscDevice<T>;
    if mdev_ptr.is_null() {
        return ENODEV.to_errno();
    }
    // SAFETY: `mdev_ptr` points to a live, registered `MiscDevice<T>` (kept
    // alive by its `Registration` for as long as the device exists).
    let mdev = unsafe { &*mdev_ptr };
    // SAFETY: VFS passes a valid `file`.
    let f = unsafe { File::from_ptr(file) };
    let init = match T::open(mdev.open_data(), f) {
        Ok(init) => init,
        Err(e) => return e.to_errno(),
    };
    let pinned = match Box::pin_init(init, GFP_KERNEL) {
        Ok(pinned) => pinned,
        Err(_) => return ENOMEM.to_errno(),
    };
    // SAFETY: `pinned` is a freshly allocated `Box`; moving it into a raw
    // pointer is safe, and the pointer is freed exactly once, in
    // `release_callback`. The value itself is never moved afterwards.
    let raw = Box::into_raw(unsafe { Pin::into_inner_unchecked(pinned) });
    // SAFETY: `file` is valid; `private_data` now owns the per-file data.
    unsafe { (*file).private_data = raw as *mut c_void };
    0
}

/// `read` trampoline: copies data to userspace and advances `*offset`.
unsafe extern "C" fn read_callback<T: FileOperations>(
    file: *mut bindings::file,
    buf: *mut c_char,
    len: usize,
    offset: *mut bindings::loff_t,
) -> isize {
    let data = match file_data::<T>(file) {
        Some(data) => data,
        None => return EFAULT.to_errno() as isize,
    };
    // SAFETY: VFS passes a valid `file`.
    let f = unsafe { File::from_ptr(file) };
    // SAFETY: `buf` is a userspace buffer of `len` bytes supplied by the VFS
    // for this read.
    let mut writer = unsafe { UserSliceWriter::new(buf as *mut c_void, len) };
    // SAFETY: VFS passes a valid `offset` pointing at the file position.
    let off = unsafe { *offset as u64 };
    match data.read(f, &mut writer, off) {
        Ok(n) => {
            // SAFETY: `offset` is valid; the driver read `n` bytes from `off`.
            unsafe { *offset = (off + n as u64) as bindings::loff_t };
            n as isize
        }
        Err(e) => e.to_errno() as isize,
    }
}

/// `write` trampoline: copies data from userspace and advances `*offset`.
unsafe extern "C" fn write_callback<T: FileOperations>(
    file: *mut bindings::file,
    buf: *const c_char,
    len: usize,
    offset: *mut bindings::loff_t,
) -> isize {
    let data = match file_data::<T>(file) {
        Some(data) => data,
        None => return EFAULT.to_errno() as isize,
    };
    // SAFETY: VFS passes a valid `file`.
    let f = unsafe { File::from_ptr(file) };
    // SAFETY: `buf` is a userspace buffer of `len` bytes supplied by the VFS
    // for this write.
    let mut reader = unsafe { UserSliceReader::new(buf as *mut c_void, len) };
    // SAFETY: VFS passes a valid `offset` pointing at the file position.
    let off = unsafe { *offset as u64 };
    match data.write(f, &mut reader, off) {
        Ok(n) => {
            // SAFETY: `offset` is valid; the driver consumed `n` bytes from `off`.
            unsafe { *offset = (off + n as u64) as bindings::loff_t };
            n as isize
        }
        Err(e) => e.to_errno() as isize,
    }
}

/// `unlocked_ioctl` / `compat_ioctl` trampoline.
unsafe extern "C" fn ioctl_callback<T: FileOperations>(
    file: *mut bindings::file,
    cmd: c_uint,
    arg: c_ulong,
) -> c_long {
    let data = match file_data::<T>(file) {
        Some(data) => data,
        None => return EFAULT.to_errno() as c_long,
    };
    // SAFETY: VFS passes a valid `file`.
    let f = unsafe { File::from_ptr(file) };
    match data.ioctl(f, cmd, arg as usize) {
        Ok(ret) => ret as c_long,
        Err(e) => e.to_errno() as c_long,
    }
}

/// `release` trampoline: runs [`FileOperations::release`] and frees the
/// per-file data.
unsafe extern "C" fn release_callback<T: FileOperations>(
    _inode: *mut bindings::inode,
    file: *mut bindings::file,
) -> c_int {
    // SAFETY: VFS passes a valid `file`. If `private_data` is non-null, it is
    // a `Box<T>` created by `open_callback` (see the module docs).
    let data = unsafe { (*file).private_data } as *mut T;
    if !data.is_null() {
        // SAFETY: `data` came from `Box::into_raw` in `open_callback` and is
        // owned by this file; reconstructing the `Box` frees it exactly once.
        let mut boxed = unsafe { Box::from_raw(data) };
        // SAFETY: VFS passes a valid `file`.
        let f = unsafe { File::from_ptr(file) };
        boxed.release(f);
        // SAFETY: `private_data` is dead from here on; clear it so a second
        // release (which the VFS never does) could not free it again.
        unsafe { (*file).private_data = core::ptr::null_mut() };
        // `boxed` drops here.
    }
    0
}
