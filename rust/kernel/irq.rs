// SPDX-License-Identifier: GPL-2.0

//! Interrupt handling.
//!
//! An interrupt is requested with [`request_threaded_irq`] (the
//! real function behind the `request_irq` static inline) and freed
//! with [`free_irq`]. [`Irq`] owns the request: dropping it frees
//! the line, so a driver that keeps it in its state can never leak
//! the registration.
//!
//! The handler is a [`IrqHandler`]; it runs in interrupt context,
//! so it takes `&self` and must not sleep (use atomics or a
//! `SpinLock` for its state, never a `Mutex`).
//!
//! A virtual IRQ (no hardware behind it) can be allocated with
//! [`alloc_virq`] (`__irq_alloc_descs`), given a dispatch handler
//! with [`set_simple_handler`] (`__irq_set_handler` +
//! `handle_simple_irq`), and fired in software with [`fire`]
//! (`generic_handle_irq`). That proves the request/handler/free
//! path without touching a hardware line; a real line uses the
//! same `request` call with its own IRQ number.

use crate::{
    bindings,
    error::{code::*, Error, Result},
    prelude::*,
    str::CStr,
};
use core::ffi::{c_int, c_ulong, c_void};

/// `IRQ_NONE`: the handler did not handle this interrupt.
pub const IRQ_NONE: u32 = 0;

/// `IRQ_HANDLED`: the handler handled this interrupt.
pub const IRQ_HANDLED: u32 = 1;

/// The return value of an interrupt handler.
pub enum IrqReturn {
    /// The interrupt was not for this device.
    None,
    /// The interrupt was handled.
    Handled,
}

impl IrqReturn {
    fn to_u32(self) -> u32 {
        match self {
            IrqReturn::None => IRQ_NONE,
            IrqReturn::Handled => IRQ_HANDLED,
        }
    }
}

/// An interrupt handler.
///
/// Runs in interrupt context: it must not sleep, and it only gets
/// `&self`, so shared state must be lock-free (atomics) or behind
/// a `SpinLock`.
pub trait IrqHandler: Send + Sync {
    /// Handle interrupt `irq`. Return [`IrqReturn::Handled`] when
    /// this device raised it.
    fn handle(&self, irq: u32) -> IrqReturn;
}

/// A requested interrupt line. Frees the line on drop.
pub struct Irq<H: IrqHandler> {
    irq: u32,
    handler: Box<H>,
}

// SAFETY: the raw pointer handed to the C core (`dev_id`) is the
// `Box<H>` contents; Rust never dereferences it except through the
// callback, which the core only calls while the line is requested
// (and `H: Send + Sync` makes sharing it across the interrupt
// context sound). The line is freed in `Drop` before the box is.
unsafe impl<H: IrqHandler> Send for Irq<H> {}
unsafe impl<H: IrqHandler> Sync for Irq<H> {}

impl<H: IrqHandler> Irq<H> {
    /// Request interrupt `irq`, calling `handler` when it fires.
    /// `flags` are the `request_irq` flags (0 for a plain line);
    /// `name` shows up in `/proc/interrupts`.
    pub fn request(irq: u32, handler: H, flags: c_ulong, name: &CStr) -> Result<Self> {
        let mut handler = Box::new(handler, GFP_KERNEL).map_err(|_| ENOMEM)?;
        let dev_id = &mut *handler as *mut H as *mut c_void;
        // SAFETY: `request_threaded_irq` is the documented
        // constructor; the handler pointer is a valid callback and
        // `dev_id` points at the live `Box<H>` contents, which
        // outlive the request (the box is stored in `Self`).
        let ret = unsafe {
            bindings::request_threaded_irq(
                irq,
                Some(irq_callback::<H>),
                None,
                flags,
                name.as_ptr(),
                dev_id,
            )
        };
        if ret != 0 {
            return Err(Error::from_errno(ret));
        }
        Ok(Self { irq, handler })
    }

    /// The IRQ number.
    pub fn irq(&self) -> u32 {
        self.irq
    }
}

impl<H: IrqHandler> Drop for Irq<H> {
    fn drop(&mut self) {
        let dev_id = &mut *self.handler as *mut H as *mut c_void;
        // SAFETY: requested by `request()` with this `dev_id`;
        // freeing releases the core's reference to the handler.
        // (`free_irq` returns the `dev_id` pointer in the
        // bindings; it is discarded here.)
        unsafe { bindings::free_irq(self.irq, dev_id) };
    }
}

unsafe extern "C" fn irq_callback<H: IrqHandler>(irq: c_int, dev_id: *mut c_void) -> u32 {
    // SAFETY: `dev_id` is the `Box<H>` contents, live while the
    // line is requested.
    let handler = unsafe { &*(dev_id as *const H) };
    handler.handle(irq as u32).to_u32()
}

/// Allocate a virtual IRQ (no hardware behind it). Returns its
/// number. Free it with [`free_virq`] after freeing any request
/// on it.
///
/// A fresh descriptor has no chip (`no_irq_chip`), and
/// `__setup_irq` rejects that with `-ENOSYS` when the line is
/// requested, so the dummy chip (no-op mask/unmask) is assigned
/// here: the line is then requestable, while still having no
/// hardware behind it.
pub fn alloc_virq(owner: *mut bindings::module) -> Result<u32> {
    // SAFETY: `__irq_alloc_descs` is the documented allocator;
    // `-1` asks for any free number, `0`/`1`/`0` mean "from 0,
    // one descriptor, any node", and the affinity is NULL.
    let ret = unsafe {
        bindings::__irq_alloc_descs(-1, 0, 1, 0, owner, core::ptr::null())
    };
    if ret < 0 {
        return Err(Error::from_errno(ret));
    }
    let irq = ret as u32;
    // SAFETY: `irq_set_chip` assigns the chip of a live
    // descriptor; `dummy_irq_chip` is the core's own no-op chip.
    // (`&raw mut` takes its address without creating a mutable
    // reference to the mutable static.)
    let ret = unsafe {
        bindings::irq_set_chip(irq, &raw mut bindings::dummy_irq_chip)
    };
    if ret != 0 {
        free_virq(irq);
        return Err(Error::from_errno(ret));
    }
    Ok(irq)
}

/// A virtual IRQ number. Frees the descriptor on drop.
///
/// The descriptor must be freed after any [`Irq`] on it: keep
/// the `Irq` field before the `Virq` field (fields drop in
/// declaration order), or free them explicitly in that order.
pub struct Virq(u32);

impl Virq {
    /// Allocate a virtual IRQ (no hardware behind it).
    pub fn alloc(owner: *mut bindings::module) -> Result<Self> {
        Ok(Self(alloc_virq(owner)?))
    }

    /// The IRQ number.
    pub fn number(&self) -> u32 {
        self.0
    }
}

impl Drop for Virq {
    fn drop(&mut self) {
        free_virq(self.0)
    }
}

/// Free a virtual IRQ allocated by [`alloc_virq`].
pub fn free_virq(irq: u32) {
    // SAFETY: allocated by `alloc_virq`; freeing releases the
    // descriptor.
    unsafe { bindings::irq_free_descs(irq, 1) }
}

/// Give a virtual IRQ the plain dispatch handler
/// (`handle_simple_irq`), so [`fire`] reaches the requested
/// handler. A fresh descriptor's handler is `handle_bad_irq`,
/// which would never call it.
pub fn set_simple_handler(irq: u32) -> Result<()> {
    // SAFETY: `__irq_set_handler` is the documented setter; the
    // handler is the core's own `handle_simple_irq`, and the line
    // is not chained and has no name. (It returns `()` in the
    // bindings, so there is nothing to check; it cannot fail for
    // a live virtual IRQ.)
    unsafe {
        bindings::__irq_set_handler(
            irq,
            Some(bindings::handle_simple_irq),
            0,
            core::ptr::null(),
        )
    };
    Ok(())
}

/// Fire interrupt `irq` in software (`generic_handle_irq`).
/// The requested handler runs synchronously in the caller's
/// context.
///
/// `generic_handle_irq` expects interrupts disabled (it is
/// normally called from interrupt context), and the core answers
/// a call with them on with a WARN_ONCE. `fire` is called from
/// process context (a `write`), where they are on, so the flags
/// are saved and restored around the call.
pub fn fire(irq: u32) -> Result<()> {
    // SAFETY: `local_irq_save` / `local_irq_restore` are the
    // documented, nesting-safe pair for disabling interrupts
    // around a short critical section; the flags are restored
    // on every path out of here.
    let flags = unsafe { bindings::local_irq_save() };
    // SAFETY: `generic_handle_irq` dispatches a live IRQ number;
    // the caller only fires numbers it allocated and requested.
    let ret = unsafe { bindings::generic_handle_irq(irq) };
    unsafe { bindings::local_irq_restore(flags) };
    if ret != 0 {
        return Err(Error::from_errno(ret));
    }
    Ok(())
}
