// SPDX-License-Identifier: GPL-2.0

//! Device-tree node access (Step 10, option 2).
//!
//! Before this module, every driver that needed the device tree
//! repeated the same two calls by hand: `of_find_node_by_name`
//! (with a NULL check, and a comment explaining why `of_node_put`
//! is not called) and then one property lookup. That repetition
//! hid the same lookup behind three slightly different shapes in
//! `gpio`, `clk`, and `bus`. `OfNode` is the one shape: look a
//! node up by name, then read its properties through methods.
//! The existing helpers now delegate to it, so their callers do
//! not change.
//!
//! `of_node_put` is deliberately not called on drop:
//! `CONFIG_OF_DYNAMIC` is off in this kernel, so it is a no-op
//! and the function is not even in the bindings. Nodes found by
//! name are static for the life of the kernel.

use crate::{bindings, error::Result, prelude::*};
use core::ffi::c_int;

/// A device-tree node, found by name. Borrowed: the node is owned
/// by the device tree, which outlives any driver, so there is no
/// `Drop` and no reference count to release.
pub struct OfNode {
    node: *mut bindings::device_node,
}

// SAFETY: the pointer is only ever handed to the OF core
// (`of_find_property`, `of_property_read_*`,
// `of_get_named_gpio_flags`); Rust never dereferences it, and
// the node is static for the life of the kernel.
unsafe impl Send for OfNode {}
unsafe impl Sync for OfNode {}

impl OfNode {
    /// Look a node up by name (`of_find_node_by_name`, from the
    /// root). Fails with `ENODEV` when there is no such node.
    pub fn find_by_name(name: &CStr) -> Result<Self> {
        // SAFETY: `of_find_node_by_name` is the documented
        // lookup; NULL means there is no such node.
        let node = unsafe {
            bindings::of_find_node_by_name(core::ptr::null_mut(), name.as_ptr())
        };
        if node.is_null() {
            return Err(ENODEV);
        }
        Ok(Self { node })
    }

    /// The raw node pointer, for passing to C functions that take
    /// one (for example `of_clk_get_from_provider`).
    pub fn as_ptr(&self) -> *mut bindings::device_node {
        self.node
    }

    /// Whether the node carries property `prop`
    /// (`of_find_property`): this is how boolean device-tree
    /// properties are tested, since they have no value to read.
    pub fn has_property(&self, prop: &CStr) -> bool {
        // SAFETY: `of_find_property` is the documented test;
        // NULL means the property is absent. The length pointer
        // may be NULL.
        let p = unsafe {
            bindings::of_find_property(self.node, prop.as_ptr(), core::ptr::null_mut())
        };
        !p.is_null()
    }

    /// Read one `u32` from property `prop` at `index`
    /// (`of_property_read_u32_index`).
    pub fn read_u32(&self, prop: &CStr, index: u32) -> Result<u32> {
        let mut out: u32 = 0;
        // SAFETY: `of_property_read_u32_index` is the documented
        // reader; it writes one `u32` to `out` on success.
        let ret = unsafe {
            bindings::of_property_read_u32_index(
                self.node,
                prop.as_ptr(),
                index,
                &mut out as *mut u32,
            )
        };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(out)
    }

    /// Resolve a GPIO from this node's `prop` property
    /// (`of_get_named_gpio_flags`). Returns the *Linux* GPIO
    /// number for entry `index`. Returns `-EPROBE_DEFER` when
    /// the GPIO chip is not registered yet; a probe that
    /// propagates that error is retried by the driver core once
    /// the chip is up.
    pub fn named_gpio(&self, prop: &CStr, index: u32) -> Result<u32> {
        // SAFETY: `of_get_named_gpio_flags` resolves the phandle
        // and offset to a Linux GPIO number; the flags pointer
        // may be NULL. It returns a negative errno (including
        // `-EPROBE_DEFER`) on failure.
        let ret = unsafe {
            bindings::of_get_named_gpio_flags(
                self.node,
                prop.as_ptr(),
                index as c_int,
                core::ptr::null_mut(),
            )
        };
        if ret < 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(ret as u32)
    }
}
