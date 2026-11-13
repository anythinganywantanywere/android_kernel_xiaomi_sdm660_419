// SPDX-License-Identifier: GPL-2.0

//! Register map abstraction (Step 11, option 2).
//!
//! A `regmap` is the kernel's generic register interface: instead
//! of talking to a bus directly (SPMI, I2C, MMIO), a driver talks
//! to a regmap, and the regmap talks to the bus. The PMIC drivers
//! in this tree (the ADC, the RTC, the regulators) all use the
//! PMIC's regmap, which is how the ADC driver reads and writes
//! its registers (`regmap_bulk_read` / `regmap_bulk_write` on
//! `adc->regmap`, base `0x3100`).
//!
//! [`Regmap`] is a borrowed handle: the regmap itself is owned
//! by the device driver that created it (the PMIC driver), and
//! it outlives any driver that borrows it (the PMIC driver is
//! built in and never unbinds). There is no `Drop` that frees
//! it. The regmap core serialises concurrent access with its
//! own lock, so a shared `Regmap` is `Send` and `Sync`.
//!
//! The usual source is a parent device: an SPMI slave's parent
//! is the PMIC, so `Regmap::from_device(sdev.parent_device())`
//! yields the PMIC regmap the slave's C driver uses.

use crate::{
    bindings,
    error::{Error, Result},
    prelude::*,
};

/// A register map, borrowed from the device that owns it.
///
/// A transparent wrapper over `*mut bindings::regmap`; the
/// regmap is owned by its creator and is only borrowed here.
#[repr(transparent)]
pub struct Regmap {
    map: *mut bindings::regmap,
}

// SAFETY: the pointer is only ever handed to the regmap core
// (`regmap_read`, `regmap_write`, `regmap_bulk_read`,
// `regmap_bulk_write`); Rust never dereferences it, and the
// regmap outlives any borrower. The regmap core serialises
// access with its own lock.
unsafe impl Send for Regmap {}
unsafe impl Sync for Regmap {}

impl Regmap {
    /// Get a device's regmap (`dev_get_regmap`, default name).
    /// Fails with `ENODEV` when the device has no regmap.
    pub fn from_device(dev: *mut bindings::device) -> Result<Self> {
        // SAFETY: `dev_get_regmap` is the documented lookup; a
        // NULL name means the default regmap. NULL means the
        // device has none.
        let map = unsafe { bindings::dev_get_regmap(dev, core::ptr::null()) };
        if map.is_null() {
            return Err(ENODEV);
        }
        Ok(Self { map })
    }

    /// Read one register (`regmap_read`). The value width is
    /// the regmap's `val_bits` (8 bits for the PMIC regmap, so
    /// the value is one byte).
    pub fn read(&self, reg: u32) -> Result<u32> {
        let mut val: u32 = 0;
        // SAFETY: `regmap_read` is the documented single
        // read; it writes one `u32` to `val` on success and
        // returns a negative errno on failure.
        let ret = unsafe { bindings::regmap_read(self.map, reg, &mut val) };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(val)
    }

    /// Write one register (`regmap_write`).
    pub fn write(&self, reg: u32, val: u32) -> Result<()> {
        // SAFETY: `regmap_write` is the documented single
        // write; it returns a negative errno on failure.
        let ret = unsafe { bindings::regmap_write(self.map, reg, val) };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    /// Read `buf.len()` bytes starting at `reg`
    /// (`regmap_bulk_read`). For the PMIC regmap (`val_bits`
    /// 8) the count is the byte count.
    pub fn read_bulk(&self, reg: u32, buf: &mut [u8]) -> Result<()> {
        // SAFETY: `regmap_bulk_read` is the documented bulk
        // read; `buf` is a live Rust slice of the length
        // passed, and the count is in values (bytes, for an
        // 8-bit regmap). It returns a negative errno on failure.
        let ret = unsafe {
            bindings::regmap_bulk_read(
                self.map,
                reg,
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                buf.len(),
            )
        };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    /// Write `buf.len()` bytes starting at `reg`
    /// (`regmap_bulk_write`). For the PMIC regmap (`val_bits`
    /// 8) the count is the byte count.
    pub fn write_bulk(&self, reg: u32, buf: &[u8]) -> Result<()> {
        // SAFETY: `regmap_bulk_write` is the documented bulk
        // write; `buf` is a live Rust slice of the length
        // passed. It returns a negative errno on failure.
        let ret = unsafe {
            bindings::regmap_bulk_write(
                self.map,
                reg,
                buf.as_ptr() as *const core::ffi::c_void,
                buf.len(),
            )
        };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }
}
