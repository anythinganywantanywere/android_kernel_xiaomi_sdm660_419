// SPDX-License-Identifier: GPL-2.0

//! I2C bus access (Step 9, option 2).
//!
//! The I2C core is a plain C API: an adapter is looked up by bus
//! number, and a transfer is a list of messages (`struct i2c_msg`)
//! handed to `i2c_transfer`, which serialises them on the bus
//! lock. There is no safe Rust I2C abstraction in this backport
//! yet; this module is the first one, deliberately thin: it wraps
//! the adapter reference and one register-read transfer, and
//! nothing else.
//!
//! A register read is the standard two-message sequence: first a
//! write of the register address, then a read of the value. The
//! bus lock serialises it against the C drivers that own the
//! devices on the same bus, so a read-only probe cannot corrupt
//! another driver's transfer.

use crate::{bindings, error::Result, prelude::*};
use core::ffi::c_int;

/// An I2C adapter, looked up by bus number (`i2c_get_adapter`).
/// The core owns the adapter; this holds a reference (`put` on
/// drop).
pub struct I2cAdapter {
    adapter: *mut bindings::i2c_adapter,
}

// SAFETY: the pointer is only ever handed to the I2C core
// (`i2c_transfer`, `i2c_put_adapter`); Rust never dereferences
// it, and the reference keeps the adapter alive.
unsafe impl Send for I2cAdapter {}
unsafe impl Sync for I2cAdapter {}

impl I2cAdapter {
    /// Look an adapter up by bus number (`i2c_get_adapter`).
    /// Fails with `ENODEV` when there is no such bus.
    pub fn get(nr: i32) -> Result<Self> {
        // SAFETY: `i2c_get_adapter` is the documented lookup;
        // NULL means there is no adapter with that number.
        let adapter = unsafe { bindings::i2c_get_adapter(nr) };
        if adapter.is_null() {
            return Err(ENODEV);
        }
        Ok(Self { adapter })
    }

    /// Read one byte from register `reg` of the device at 7-bit
    /// address `addr`, as a two-message transfer (write the
    /// register address, read the value). May sleep (it takes the
    /// bus lock), so only call it from process context.
    pub fn read_reg(&self, addr: u16, reg: u8) -> Result<u8> {
        let mut reg_buf = [reg];
        let mut val_buf = [0u8; 1];
        // SAFETY: both messages point at live one-byte kernel
        // buffers; `i2c_transfer` copies them under the bus
        // lock and returns before either buffer is dropped.
        let mut msgs = [
            bindings::i2c_msg {
                addr,
                flags: 0,
                len: 1,
                buf: reg_buf.as_mut_ptr(),
            },
            bindings::i2c_msg {
                addr,
                flags: bindings::I2C_M_RD as u16,
                len: 1,
                buf: val_buf.as_mut_ptr(),
            },
        ];
        let ret = unsafe {
            bindings::i2c_transfer(self.adapter, msgs.as_mut_ptr(), msgs.len() as c_int)
        };
        if ret < 0 {
            return Err(Error::from_errno(ret));
        }
        if ret as usize != msgs.len() {
            return Err(EIO);
        }
        Ok(val_buf[0])
    }
}

impl Drop for I2cAdapter {
    fn drop(&mut self) {
        // SAFETY: this is the reference taken by `get`; the
        // adapter is still registered (the core keeps it).
        unsafe { bindings::i2c_put_adapter(self.adapter) };
    }
}
