// SPDX-License-Identifier: GPL-2.0

//! Rust misc device sample.
//!
//! Registers `/dev/rust_miscdev`:
//! - `read` returns the contents of a small device-wide buffer, or the
//!   greeting `rust-miscdev v1` when the buffer is still empty. The buffer
//!   is shared by every file opened on the device, so a `write` from one
//!   open is visible to a later `read` from another open.
//! - `write` stores up to 128 bytes into the device-wide buffer.
//! - ioctl `RUST_MISCDEV_CLEAR` empties the buffer, ioctl
//!   `RUST_MISCDEV_GET_VERSION` reports the driver version (a `u32`).

use kernel::{
    c_str,
    file::{File, FileOperations, UserSliceReader, UserSliceWriter},
    miscdevice::{Registration, DYNAMIC_MINOR},
    prelude::*,
    sync::{new_mutex, Arc, Mutex, UniqueArc},
};

module! {
    type: RustMiscdev,
    name: "rust_miscdev",
    author: "MayaKernel",
    description: "Rust misc device sample",
    license: "GPL",
}

/// Maximum bytes kept in the device buffer.
const MAX_BUF: usize = 128;

/// ioctl: clear the device buffer.
const RUST_MISCDEV_CLEAR: u32 = 0xC001;

/// ioctl: report the driver version (a `u32`) to userspace.
const RUST_MISCDEV_GET_VERSION: u32 = 0xC002;

/// Driver version reported by `RUST_MISCDEV_GET_VERSION`.
const RUST_MISCDEV_VERSION: u32 = 1;

/// Greeting returned by `read` on a fresh file.
const GREETING: &[u8] = b"rust-miscdev v1\n";

struct RustMiscdev {
    _reg: Registration<RustMiscdevOps>,
}

/// Device-wide state, shared by every file opened on the device.
///
/// Created once at module init and reference-counted with [`Arc`]: each
/// `open` clones the [`Arc`], so the buffer outlives any single file and is
/// freed only when the last file closes and the device deregisters.
#[pin_data]
struct Shared {
    #[pin]
    buf: Mutex<Vec<u8>>,
}

/// Per-file data: just a handle to the shared device state.
#[pin_data]
struct RustMiscdevOps {
    shared: Arc<Shared>,
}

impl FileOperations for RustMiscdevOps {
    type OpenData = Arc<Shared>;

    fn open(context: &Arc<Shared>, _file: &File) -> Result<impl PinInit<Self>> {
        Ok(pin_init!(Self {
            shared: context.clone(),
        }))
    }

    fn read(&self, _file: &File, writer: &mut UserSliceWriter, offset: u64) -> Result<usize> {
        let buf = self.shared.buf.lock();
        let data: &[u8] = if buf.is_empty() { GREETING } else { &buf };
        let offset = offset as usize;
        if offset >= data.len() {
            return Ok(0);
        }
        writer.write(&data[offset..])
    }

    fn write(&self, _file: &File, reader: &mut UserSliceReader, _offset: u64) -> Result<usize> {
        let mut tmp = [0u8; MAX_BUF];
        let n = reader.read(&mut tmp)?;
        let mut buf = self.shared.buf.lock();
        buf.clear();
        for &b in &tmp[..n] {
            buf.push(b, GFP_KERNEL).map_err(|_| ENOMEM)?;
        }
        Ok(n)
    }

    fn ioctl(&self, _file: &File, cmd: u32, arg: usize) -> Result<isize> {
        match cmd {
            RUST_MISCDEV_CLEAR => {
                self.shared.buf.lock().clear();
                Ok(0)
            }
            RUST_MISCDEV_GET_VERSION => {
                // SAFETY: `arg` is a userspace pointer to a `u32` supplied by
                // the caller of this ioctl.
                let mut writer = unsafe {
                    UserSliceWriter::new(
                        arg as *mut core::ffi::c_void,
                        core::mem::size_of::<u32>(),
                    )
                };
                writer.write_all(&RUST_MISCDEV_VERSION.to_ne_bytes())?;
                Ok(0)
            }
            _ => Err(ENOTTY),
        }
    }
}

impl kernel::Module for RustMiscdev {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("rust_miscdev: registering /dev/rust_miscdev\n");
        let shared: Arc<Shared> = UniqueArc::new_uninit(GFP_KERNEL)
            .map_err(|_| ENOMEM)?
            .pin_init_with(pin_init!(Shared {
                buf <- new_mutex!(Vec::new(), "rust_miscdev_buf"),
            }))?
            .into();
        let reg = Registration::<RustMiscdevOps>::new(
            c_str!("rust_miscdev"),
            DYNAMIC_MINOR,
            shared,
            module.as_ptr(),
        )?;
        Ok(Self { _reg: reg })
    }
}
