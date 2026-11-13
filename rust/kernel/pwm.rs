// SPDX-License-Identifier: GPL-2.0

//! Qualcomm PWM LPG (Light Pulse Generator) reader.
//!
//! The PMIC carries LPG blocks (`qcom,pwm-lpg`) that drive the
//! display backlight and the notification LED. Each LPG channel
//! occupies `0x100` bytes, and the PWM duty is a 9-bit value
//! split over two registers. This is a read-only wrapper: it
//! never writes, so it cannot change the brightness or the LED.
//!
//! Register map (`drivers/pwm/pwm-qti-lpg.c`):
//!
//! * Channel `N` (1-based) sits at `base + (N - 1) * 0x100`
//!   (`REG_SIZE_PER_LPG`), where `base` is the `lpg-base`
//!   from the device tree.
//! * `REG_LPG_PWM_VALUE_LSB` (`0x44`) holds duty bits 7:0;
//!   `REG_LPG_PWM_VALUE_MSB` (`0x45`) holds duty bit 8 in its
//!   bit 0 (`LPG_PWM_VALUE_MSB_MASK`).
//! * `REG_LPG_ENABLE_CONTROL` (`0x46`) holds the output
//!   enable in bit 7 (`LPG_EN_LPG_OUT_BIT`).
//!
//! The reads go through an `SpmiDevice` handle, because the
//! LPG lives on the PMIC and its C driver already owns it.

use crate::{error::Result, spmi::SpmiDevice};

/// Bytes per LPG channel (`REG_SIZE_PER_LPG`).
const REG_SIZE_PER_LPG: u16 = 0x100;
/// PWM duty low byte (`REG_LPG_PWM_VALUE_LSB`).
const REG_PWM_VALUE_LSB: u16 = 0x44;
/// PWM duty high bit (`REG_LPG_PWM_VALUE_MSB`).
const REG_PWM_VALUE_MSB: u16 = 0x45;
/// Output-enable register (`REG_LPG_ENABLE_CONTROL`).
const REG_ENABLE_CONTROL: u16 = 0x46;
/// Output-enable bit in `REG_ENABLE_CONTROL`.
const ENABLE_OUT_BIT: u8 = 0x80;
/// Duty bit 8 in `REG_PWM_VALUE_MSB`.
const VALUE_MSB_MASK: u8 = 0x01;

/// One LPG channel, read through an SPMI handle to the PMIC.
///
/// The handle is borrowed: the caller owns the `SpmiDevice`
/// and this type only reads through it.
pub struct PwmLpg<'a> {
    sdev: &'a SpmiDevice,
    reg_base: u16,
}

impl<'a> PwmLpg<'a> {
    /// Open LPG channel `id` (1-based) on the LPG block at
    /// `base`. The channel id is indexed from 1 in hardware.
    pub fn new(sdev: &'a SpmiDevice, base: u16, id: u16) -> Self {
        Self {
            sdev,
            reg_base: base + (id - 1) * REG_SIZE_PER_LPG,
        }
    }

    /// Read the 9-bit PWM duty value (0-511).
    pub fn value(&self) -> Result<u16> {
        let lsb = self.sdev.read_u8(self.reg_base + REG_PWM_VALUE_LSB)?;
        let msb = self.sdev.read_u8(self.reg_base + REG_PWM_VALUE_MSB)?;
        Ok(((msb & VALUE_MSB_MASK) as u16) << 8 | lsb as u16)
    }

    /// Read whether the LPG output is enabled.
    pub fn is_enabled(&self) -> Result<bool> {
        let v = self.sdev.read_u8(self.reg_base + REG_ENABLE_CONTROL)?;
        Ok(v & ENABLE_OUT_BIT != 0)
    }
}
