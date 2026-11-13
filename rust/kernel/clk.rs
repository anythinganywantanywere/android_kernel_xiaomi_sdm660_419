// SPDX-License-Identifier: GPL-2.0

//! Clock handling.
//!
//! A clock is obtained with [`Clk::get`] (`clk_get`, the standard
//! driver path: a device plus a connection id) or with
//! [`Clk::of_get`] (`of_clk_get` on a device-tree node, for clocks
//! that have no `clkdev` lookup entry -- on this DT-based system
//! none do, so the proof uses the OF path). [`Clk`] owns the
//! reference: dropping it calls `clk_put`, so a driver that keeps
//! it in its state can never leak the clock.
//!
//! `enable`/`disable` wrap `clk_prepare` + `clk_enable` /
//! `clk_disable` + `clk_unprepare` (the two static inlines
//! `clk_prepare_enable` / `clk_disable_unprepare` cannot be
//! called from Rust); they may sleep, so they must run in
//! process context (probe is fine, an interrupt handler is not).

use crate::{
    bindings,
    error::{code::*, from_err_ptr, Error, Result},
    str::CStr,
};
use core::ffi::c_int;

/// A clock reference. Puts the clock on drop.
pub struct Clk {
    ptr: *mut bindings::clk,
}

// SAFETY: the pointer is only ever handed to the C clock core
// (`clk_prepare_enable` / `clk_disable_unprepare` / `clk_get_rate`
// / `clk_put`); Rust never dereferences it, and the reference is
// owned by this struct until `Drop`.
unsafe impl Send for Clk {}
unsafe impl Sync for Clk {}

impl Clk {
    /// Get a clock by device and connection id (`clk_get`).
    /// `dev` may be NULL for a system-wide lookup, but on a
    /// DT-based system that only finds clocks with a `clkdev`
    /// entry.
    pub fn get(dev: *mut bindings::device, id: &CStr) -> Result<Self> {
        // SAFETY: `clk_get` is the documented constructor; it
        // returns an `ERR_PTR` on failure.
        let ptr = from_err_ptr(unsafe { bindings::clk_get(dev, id.as_ptr()) })?;
        Ok(Self { ptr })
    }

    /// Get a clock from a device-tree *consumer* node by name and
    /// index (`of_find_node_by_name` + `of_clk_get`). Used for
    /// the RNG: the `qrng` node has a `clocks` property, so
    /// `of_clk_get` resolves index 0 to the interface clock.
    /// (The provider form is [`Clk::of_get`]; a provider node has
    /// no `clocks` property and needs the phandle arguments
    /// built by hand.)
    ///
    /// `of_node_put` is not called: `CONFIG_OF_DYNAMIC` is off
    /// in this kernel, so it is a no-op.
    pub fn of_consumer_get(node_name: &CStr, index: c_int) -> Result<Self> {
        let node = crate::of::OfNode::find_by_name(node_name)?;
        // SAFETY: `of_clk_get` is the documented constructor for
        // a clock from a consumer node's `clocks` property; it
        // returns an `ERR_PTR` on failure.
        let ptr = from_err_ptr(unsafe { bindings::of_clk_get(node.as_ptr(), index) })?;
        Ok(Self { ptr })
    }

    /// Get a clock from a device-tree provider node by name.
    /// Used for the proof: the `xo_board` fixed clock has no
    /// `clkdev` entry, so `get` cannot find it.
    ///
    /// `of_clk_get` cannot be used here: it is for a *consumer*
    /// node (it parses the node's "clocks" property), and a
    /// provider node such as `xo_board` has none, so it fails
    /// with `-ENOENT`. Instead the phandle arguments are built
    /// directly -- the provider has `#clock-cells = <0>`, so
    /// there are no arguments -- and the provider is asked with
    /// `of_clk_get_from_provider`.
    ///
    /// `of_node_put` is not called: `CONFIG_OF_DYNAMIC` is off
    /// in this kernel, so it is a no-op.
    pub fn of_get(node_name: &CStr) -> Result<Self> {
        let node = crate::of::OfNode::find_by_name(node_name)?;
        // SAFETY: `of_clk_get_from_provider` is the documented
        // lookup for a clock from its provider; the arguments are
        // a live node with no clock cells. It returns an
        // `ERR_PTR` on failure.
        let mut clkspec = bindings::of_phandle_args {
            np: node.as_ptr(),
            args_count: 0,
            args: [0; 16],
        };
        let ptr = from_err_ptr(unsafe {
            bindings::of_clk_get_from_provider(&mut clkspec)
        })?;
        Ok(Self { ptr })
    }

    /// Enable the clock. May sleep.
    ///
    /// `clk_prepare_enable` is a static inline in this tree, so
    /// the two steps it wraps are called directly: `clk_prepare`
    /// then `clk_enable` (and `clk_unprepare` again when the
    /// enable fails).
    pub fn enable(&self) -> Result<()> {
        // SAFETY: the clock is a live reference owned by `Self`.
        let ret = unsafe { bindings::clk_prepare(self.ptr) };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        // SAFETY: same as above.
        let ret = unsafe { bindings::clk_enable(self.ptr) };
        if ret != 0 {
            unsafe { bindings::clk_unprepare(self.ptr) };
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    /// Disable the clock. May sleep.
    ///
    /// `clk_disable_unprepare` is a static inline in this tree,
    /// so the two steps it wraps are called directly.
    pub fn disable(&self) {
        // SAFETY: same as [`Clk::enable`].
        unsafe { bindings::clk_disable(self.ptr) };
        unsafe { bindings::clk_unprepare(self.ptr) };
    }

    /// The clock rate in Hz (`clk_get_rate`).
    pub fn rate(&self) -> u64 {
        // SAFETY: same as [`Clk::enable`].
        unsafe { bindings::clk_get_rate(self.ptr) }
    }
}

impl Drop for Clk {
    fn drop(&mut self) {
        // SAFETY: obtained by `get`/`of_get`; putting releases
        // the reference.
        unsafe { bindings::clk_put(self.ptr) }
    }
}
