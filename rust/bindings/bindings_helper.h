/* SPDX-License-Identifier: GPL-2.0 */
/*
 * Only the headers the minimal module uses. Dropped from the
 * 6.12 reference: kunit, blk-mq, blkdev, ethtool, firmware,
 * mdio, phy, wait, workqueue, and the BLK_FEAT constants.
 */

#include <linux/err.h>
#include <linux/jiffies.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/refcount.h>
#include <linux/sched.h>
#include <linux/slab.h>
#include <linux/spinlock.h>
#include <linux/string.h>
#include <linux/types.h>

/* `bindgen` gets confused at certain things. */
const size_t RUST_CONST_HELPER_ARCH_SLAB_MINALIGN = ARCH_SLAB_MINALIGN;
const size_t RUST_CONST_HELPER_PAGE_SIZE = PAGE_SIZE;
const gfp_t RUST_CONST_HELPER_GFP_ATOMIC = GFP_ATOMIC;
const gfp_t RUST_CONST_HELPER_GFP_KERNEL = GFP_KERNEL;
const gfp_t RUST_CONST_HELPER_GFP_KERNEL_ACCOUNT = GFP_KERNEL_ACCOUNT;
const gfp_t RUST_CONST_HELPER_GFP_NOWAIT = GFP_NOWAIT;
const gfp_t RUST_CONST_HELPER___GFP_ZERO = __GFP_ZERO;
const gfp_t RUST_CONST_HELPER___GFP_HIGHMEM = ___GFP_HIGHMEM;
