// SPDX-License-Identifier: GPL-2.0

//! MSM bus scaling (NoC bandwidth votes).
//!
//! Some Qualcomm peripherals sit behind a Network-on-Chip path
//! that is only powered when a driver votes for bandwidth on it.
//! The device tree describes the vote with `qcom,msm-bus`
//! properties (`name`, `num-cases`, `num-paths`,
//! `vectors-KBps`); the C driver for the same hardware registers
//! a bus client from those properties and requests a vote around
//! every register access, and releases it afterwards. A register
//! access without the vote faults with an asynchronous SError,
//! because the bus path to the peripheral is not active.
//!
//! [`BusClient`] owns such a client: it registers in `probe` (from
//! the device-tree node's bus properties, via
//! `msm_bus_pdata_from_node`) and unregisters on drop. [`BusVote`]
//! is the RAII guard for one vote: it requests the bandwidth on
//! creation and releases it on drop, so a register access is
//! always bracketed by a vote even when the access returns early
//! or errors.

use crate::{
    bindings,
    error::{code::*, Result},
};
use core::ffi::c_uint;

/// The bus-vote index for "no vote" (the bus is idle).
const VOTE_NONE: c_uint = 0;
/// The bus-vote index for the active case (100 KHz on the QRNG
/// node: the second entry of its `vectors-KBps`, `<1 618 0 800>`).
const VOTE_ACTIVE: c_uint = 1;

/// A registered MSM bus-scaling client. Unregisters on drop.
pub struct BusClient {
    client: u32,
}

// SAFETY: the client handle is only ever passed to the C bus core
// (`msm_bus_scale_client_update_request` /
// `msm_bus_scale_unregister_client`); Rust never dereferences it.
unsafe impl Send for BusClient {}
unsafe impl Sync for BusClient {}

impl BusClient {
    /// Register a bus client from a device-tree node's
    /// `qcom,msm-bus` properties.
    ///
    /// `pdev` is the platform device the client is registered
    /// for (its `struct device` is used for the allocation);
    /// `of_node` is the device-tree node carrying the bus
    /// properties (it need not be `pdev`'s own node).
    /// Returns `ENODEV` when the node has no bus properties or
    /// the registration fails.
    pub fn register(
        pdev: *mut bindings::platform_device,
        of_node: *mut bindings::device_node,
    ) -> Result<Self> {
        // SAFETY: `msm_bus_pdata_from_node` reads the bus
        // properties from `of_node` and allocates the pdata
        // against `pdev`'s device; NULL means the node carries
        // no usable bus description.
        let pdata = unsafe { bindings::msm_bus_pdata_from_node(pdev, of_node) };
        if pdata.is_null() {
            return Err(ENODEV);
        }
        // SAFETY: `pdata` is a live bus-scaling description
        // returned by the bus core; the client handle is 0 when
        // the registration fails.
        let client = unsafe { bindings::msm_bus_scale_register_client(pdata) };
        if client == 0 {
            return Err(ENODEV);
        }
        Ok(Self { client })
    }

    /// Request a bandwidth vote. `index` selects the use case
    /// from the node's `vectors-KBps` (0 = no vote, 1 = the
    /// active case).
    pub fn request(&self, index: c_uint) -> Result<()> {
        // SAFETY: `client` is a live handle owned by `Self`.
        let ret = unsafe { bindings::msm_bus_scale_client_update_request(self.client, index) };
        if ret != 0 {
            return Err(crate::error::Error::from_errno(ret));
        }
        Ok(())
    }

    /// Vote for the active case (index 1). See [`BusClient::request`].
    pub fn vote(&self) -> Result<()> {
        self.request(VOTE_ACTIVE)
    }

    /// Release the vote (index 0). See [`BusClient::request`].
    pub fn unvote(&self) -> Result<()> {
        self.request(VOTE_NONE)
    }

    /// Request the active vote and return a guard that releases
    /// it on drop, so a register access is always bracketed by
    /// a vote.
    pub fn vote_guard(&self) -> Result<BusVote<'_>> {
        self.vote()?;
        Ok(BusVote { client: self })
    }
}

impl Drop for BusClient {
    fn drop(&mut self) {
        // SAFETY: `client` is a live handle owned by `Self`.
        unsafe { bindings::msm_bus_scale_unregister_client(self.client) };
    }
}

/// An RAII guard for one bus vote: it requests the active vote on
/// creation (see [`BusClient::vote_guard`]) and releases it when
/// it drops.
pub struct BusVote<'a> {
    client: &'a BusClient,
}

impl Drop for BusVote<'_> {
    fn drop(&mut self) {
        let _ = self.client.unvote();
    }
}
