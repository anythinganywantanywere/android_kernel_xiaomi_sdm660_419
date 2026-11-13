// SPDX-License-Identifier: GPL-2.0

//! Small text helpers for rendering device snapshots (Step 10
//! cleanup, item 1).
//!
//! Before this module, `copy_bytes`, `write_u32_dec`,
//! `write_i32_dec`, `write_u32_hex` and `write_line` were
//! copy-pasted into every sample that serves a text snapshot
//! (`rust_hwmon`, `rust_thermal`, `rust_power`, `rust_i2c`,
//! `rust_gpio_irq`). Five copies of the same forty lines meant a
//! bug in one helper had to be fixed in five places. They live
//! here now, and the samples import them.

/// Copy bytes into `out`, returning how many were copied.
pub fn copy_bytes(out: &mut [u8], data: &[u8]) -> usize {
    let n = core::cmp::min(out.len(), data.len());
    out[..n].copy_from_slice(&data[..n]);
    n
}

/// Write a `u32` as decimal ASCII into `out`. Returns how many
/// bytes were written.
pub fn write_u32_dec(out: &mut [u8], mut value: u32) -> usize {
    // Decimal digits, reversed, then written in order.
    let mut digits = [0u8; 10];
    let mut count = 0usize;
    if value == 0 {
        if !out.is_empty() {
            out[0] = b'0';
            return 1;
        }
        return 0;
    }
    while value > 0 && count < digits.len() {
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
        count += 1;
    }
    let mut n = 0usize;
    while count > 0 && n < out.len() {
        count -= 1;
        out[n] = digits[count];
        n += 1;
    }
    n
}

/// Write an `i32` as decimal ASCII into `out` (handles
/// negatives). Returns how many bytes were written.
pub fn write_i32_dec(out: &mut [u8], value: i32) -> usize {
    let mut n = 0usize;
    let mut v = value;
    if v < 0 {
        if n < out.len() {
            out[n] = b'-';
            n += 1;
        }
        v = v.wrapping_neg();
    }
    n += write_u32_dec(&mut out[n..], v as u32);
    n
}

/// Write a `u32` as lowercase hex ASCII into `out` (no `0x`
/// prefix). Returns how many bytes were written.
pub fn write_u32_hex(out: &mut [u8], value: u32) -> usize {
    // Hex digits, reversed, then written in order.
    let mut digits = [0u8; 8];
    let mut count = 0usize;
    let mut v = value;
    if v == 0 {
        if !out.is_empty() {
            out[0] = b'0';
            return 1;
        }
        return 0;
    }
    while v > 0 && count < digits.len() {
        let d = (v & 0xf) as u8;
        digits[count] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        v >>= 4;
        count += 1;
    }
    let mut n = 0usize;
    while count > 0 && n < out.len() {
        count -= 1;
        out[n] = digits[count];
        n += 1;
    }
    n
}

/// Append `key=value\n` for a `u32` value, formatted as decimal.
/// Returns the number of bytes appended.
pub fn write_line(out: &mut [u8], key: &[u8], value: u32) -> usize {
    let mut n = 0usize;
    n += copy_bytes(&mut out[n..], key);
    n += write_u32_dec(&mut out[n..], value);
    n += copy_bytes(&mut out[n..], b"\n");
    n
}
