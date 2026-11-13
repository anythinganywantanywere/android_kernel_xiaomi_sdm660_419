// SPDX-License-Identifier: GPL-2.0

//! Regulator handling.
//!
//! A regulator is obtained with [`Regulator::get`]
//! (`regulator_get` with a NULL device: the lookup falls back to
//! matching the regulator by name, so a DT-registered regulator
//! such as `pm660_l1` is found by its `regulator-name`).
//! [`Regulator`] owns the reference: dropping it calls
//! `regulator_put`, so a driver that keeps it in its state can
//! never leak the regulator.
//!
//! `enable`/`disable` are `regulator_enable` /
//! `regulator_disable`; they may sleep, so they must run in
//! process context (probe is fine, an interrupt handler is not).

use crate::{
    bindings,
    error::{from_err_ptr, Error, Result},
    str::CStr,
};
use core::ffi::c_int;

/// A regulator reference. Puts the regulator on drop.
pub struct Regulator {
    ptr: *mut bindings::regulator,
}

// SAFETY: the pointer is only ever handed to the C regulator core
// (`regulator_enable` / `regulator_disable` /
// `regulator_get_voltage` / `regulator_is_enabled` /
// `regulator_put`); Rust never dereferences it, and the reference
// is owned by this struct until `Drop`.
unsafe impl Send for Regulator {}
unsafe impl Sync for Regulator {}

impl Regulator {
    /// Get a regulator by name (`regulator_get` with a NULL
    /// device). The name is the `regulator-name` from the device
    /// tree.
    pub fn get(name: &CStr) -> Result<Self> {
        // SAFETY: `regulator_get` is the documented constructor;
        // it returns an `ERR_PTR` on failure.
        let ptr = from_err_ptr(unsafe {
            bindings::regulator_get(core::ptr::null_mut(), name.as_ptr())
        })?;
        Ok(Self { ptr })
    }

    /// Enable the regulator (`regulator_enable`). May sleep.
    /// Enabling an already-enabled regulator only bumps its
    /// use count.
    pub fn enable(&self) -> Result<()> {
        // SAFETY: the regulator is a live reference owned by `Self`.
        let ret = unsafe { bindings::regulator_enable(self.ptr) };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    /// Disable the regulator (`regulator_disable`). May sleep.
    pub fn disable(&self) -> Result<()> {
        // SAFETY: same as [`Regulator::enable`].
        let ret = unsafe { bindings::regulator_disable(self.ptr) };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    /// The regulator voltage in microvolts
    /// (`regulator_get_voltage`).
    pub fn voltage(&self) -> Result<c_int> {
        // SAFETY: same as [`Regulator::enable`].
        let ret = unsafe { bindings::regulator_get_voltage(self.ptr) };
        if ret < 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(ret)
    }

    /// Whether the regulator is enabled
    /// (`regulator_is_enabled`).
    pub fn is_enabled(&self) -> Result<bool> {
        // SAFETY: same as [`Regulator::enable`].
        let ret = unsafe { bindings::regulator_is_enabled(self.ptr) };
        if ret < 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(ret != 0)
    }
}

impl Drop for Regulator {
    fn drop(&mut self) {
        // SAFETY: obtained by `get`; putting releases the reference.
        unsafe { bindings::regulator_put(self.ptr) }
    }
}
