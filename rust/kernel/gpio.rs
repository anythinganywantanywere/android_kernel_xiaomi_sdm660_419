// SPDX-License-Identifier: GPL-2.0

//! GPIO handling.
//!
//! A GPIO is addressed by its number (`gpio_to_desc`) and driven
//! with the descriptor API (`gpiod_direction_output`,
//! `gpiod_set_value`, `gpiod_get_value`). [`Gpio`] is a borrowed
//! wrapper over the descriptor: the GPIO chip owns the descriptor,
//! so there is nothing to put -- the pin only needs its direction
//! and value, which the driver sets explicitly.
//!
//! The number `gpio_to_desc` takes is the *Linux* GPIO number,
//! not the hardware offset in the device tree: the TLMM chip gets
//! a dynamic base (allocated from the top of the GPIO space), so
//! the DT's `&tlmm 64` is not Linux GPIO 64. Resolve a DT GPIO
//! with [`number_from_of`] (`of_get_named_gpio_flags`), which
//! returns the Linux number for a node's `gpios` entry; the base
//! of a chip then follows from any one of its resolved numbers,
//! so an unreferenced offset of the same chip is
//! `resolved - offset + wanted`.
//!
//! For a driver bound to a device-tree node, `gpiod_get` (device
//! plus connection id) is the usual constructor; a synthetic
//! device has no GPIO lookup, so the proof uses the OF lookup
//! above.

use crate::{
    bindings,
    error::{code::*, Error, Result},
    str::CStr,
};
use core::ffi::c_int;

/// A GPIO, addressed by number. Borrowed: the GPIO chip owns the
/// descriptor, so there is no `Drop`.
pub struct Gpio {
    desc: *mut bindings::gpio_desc,
}

// SAFETY: the pointer is only ever handed to the C GPIO core
// (`gpiod_*`); Rust never dereferences it, and the descriptor is
// owned by the GPIO chip, which outlives any driver.
unsafe impl Send for Gpio {}
unsafe impl Sync for Gpio {}

impl Gpio {
    /// Look a GPIO up by number (`gpio_to_desc`). The number is
    /// the *Linux* GPIO number, not the hardware offset; see
    /// [`number_from_of`] to resolve a device-tree GPIO.
    /// Fails with `ENODEV` when the number is not a GPIO of any
    /// chip.
    pub fn from_number(gpio: u32) -> Result<Self> {
        // SAFETY: `gpio_to_desc` is the documented lookup; NULL
        // means the number is not a GPIO.
        let desc = unsafe { bindings::gpio_to_desc(gpio) };
        if desc.is_null() {
            return Err(ENODEV);
        }
        Ok(Self { desc })
    }

    /// Configure the pin as an output driving `value`
    /// (`gpiod_direction_output`). May sleep.
    pub fn direction_output(&self, value: bool) -> Result<()> {
        // SAFETY: the descriptor is a live GPIO from `from_number`.
        let ret = unsafe { bindings::gpiod_direction_output(self.desc, value as c_int) };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    /// Configure the pin as an input (`gpiod_direction_input`).
    /// May sleep.
    pub fn direction_input(&self) -> Result<()> {
        // SAFETY: same as [`Gpio::direction_output`].
        let ret = unsafe { bindings::gpiod_direction_input(self.desc) };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(())
    }

    /// Drive the pin (`gpiod_set_value`). Only valid on an output.
    pub fn set_value(&self, value: bool) {
        // SAFETY: same as [`Gpio::direction_output`].
        unsafe { bindings::gpiod_set_value(self.desc, value as c_int) }
    }

    /// Read the pin (`gpiod_get_value`).
    pub fn get_value(&self) -> Result<bool> {
        // SAFETY: same as [`Gpio::direction_output`].
        let ret = unsafe { bindings::gpiod_get_value(self.desc) };
        if ret < 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(ret != 0)
    }

    /// The interrupt number for this pin (`gpiod_to_irq`), if the
    /// pin can raise one. Returns `ENXIO` when the pin has no IRQ.
    /// The caller owns the returned line (request it, then free
    /// it); the GPIO itself stays borrowed from its chip.
    pub fn to_irq(&self) -> Result<u32> {
        // SAFETY: same as [`Gpio::direction_output`]; a negative
        // return means the pin has no IRQ.
        let ret = unsafe { bindings::gpiod_to_irq(self.desc as *const _) };
        if ret < 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(ret as u32)
    }
}

/// Resolve a GPIO from a device-tree node
/// (`OfNode::named_gpio`). Returns the *Linux* GPIO number for
/// entry `index` of the node's `prop` property (for example the
/// `camera_focus` key's `gpios` entry).
///
/// Returns `-EPROBE_DEFER` when the GPIO chip is not registered
/// yet; a probe that propagates that error is retried by the
/// driver core once the chip is up.
pub fn number_from_of(node_name: &CStr, prop: &CStr, index: u32) -> Result<u32> {
    crate::of::OfNode::find_by_name(node_name)?.named_gpio(prop, index)
}
