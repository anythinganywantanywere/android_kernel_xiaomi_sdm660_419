// SPDX-License-Identifier: GPL-2.0

//! Memory-mapped I/O.
//!
//! A driver that talks to hardware reads and writes its registers
//! through a mapping created by [`ioremap`]. [`IoMem`] owns such a
//! mapping: it maps on construction and unmaps on drop, so a driver
//! that keeps it in its state can never leak the mapping.
//!
//! The C accessors (`ioremap`, `iounmap`, `readl`, `writel`) are
//! static inlines in this tree, so they are reached through the
//! `rust_helper_*` wrappers in `rust/helpers/io.c` and appear here
//! as `bindings::ioremap` etc.

use crate::{
    bindings,
    error::{code::*, Result},
};
use core::ffi::c_void;

/// A memory-mapped I/O region, created by [`ioremap`].
///
/// The mapping covers `size` bytes starting at the physical address
/// given to [`IoMem::map`]. `readl`/`writel` take a byte offset from
/// the base; the caller must keep the offset inside the mapping and
/// 4-byte aligned, as a real register access would be.
pub struct IoMem {
    ptr: *mut c_void,
    size: usize,
}

// SAFETY: the pointer is only ever used for `readl`/`writel` on a
// live mapping (or handed to `iounmap` in `Drop`); Rust never
// dereferences it as a Rust reference, and the mapping is owned by
// this struct until then. The underlying pages are device memory,
// shared with nothing Rust owns.
unsafe impl Send for IoMem {}
unsafe impl Sync for IoMem {}

impl IoMem {
    /// Map `size` bytes at physical address `phys`.
    ///
    /// Fails with `ENOMEM` when the mapping cannot be created.
    pub fn map(phys: u64, size: usize) -> Result<Self> {
        // SAFETY: `ioremap` is the documented constructor for a
        // mapping; a NULL return means it failed, and the error is
        // reported instead of being dereferenced.
        let ptr = unsafe { bindings::ioremap(phys, size) };
        if ptr.is_null() {
            return Err(ENOMEM);
        }
        Ok(Self { ptr, size })
    }

    /// The size of the mapping, in bytes.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Read a 32-bit register at `offset` bytes from the base.
    pub fn readl(&self, offset: usize) -> u32 {
        // SAFETY: `ptr` is a live mapping of `size` bytes; the
        // caller keeps `offset` inside it, and a 32-bit read at a
        // 4-byte aligned offset is what the hardware expects.
        unsafe { bindings::readl(self.ptr.add(offset)) }
    }

    /// Write a 32-bit register at `offset` bytes from the base.
    pub fn writel(&self, value: u32, offset: usize) {
        // SAFETY: same as [`IoMem::readl`], for a write.
        unsafe { bindings::writel(value, self.ptr.add(offset)) }
    }
}

impl Drop for IoMem {
    fn drop(&mut self) {
        // SAFETY: the mapping was created by `map()` and is owned
        // by this struct; unmapping releases it.
        unsafe { bindings::iounmap(self.ptr) }
    }
}
