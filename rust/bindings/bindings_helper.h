/* SPDX-License-Identifier: GPL-2.0 */
/*
 * Only the headers the minimal module uses. Dropped from the
 * 6.12 reference: kunit, blk-mq, blkdev, ethtool, firmware,
 * mdio, phy, wait, workqueue, and the BLK_FEAT constants.
 *
 * Added for the file/miscdevice abstractions: fs.h and
 * miscdevice.h.
 *
 * Added for the platform driver abstraction: platform_device.h.
 *
 * Added for the resource abstractions (Steps 4-7): clk.h and of.h
 * (clock), gpio.h and gpio/consumer.h (GPIO), interrupt.h and irq.h
 * (interrupt), regulator/consumer.h (regulator).
 *
 * Added for Step 11: spmi.h (SPMI bus, PMIC access) and
 * regmap.h (register map, used by the PMIC drivers).
 * `of_platform.h` is for `of_find_device_by_node`, which the
 * RTC and battery drivers use to find the PMIC device (whose
 * regmap they borrow) without binding a second driver to a
 * device the C drivers already own.
 */

#include <linux/clk.h>
#include <linux/err.h>
#include <linux/fs.h>
#include <linux/gpio.h>
#include <linux/gpio/consumer.h>
#include <linux/i2c.h>
#include <linux/interrupt.h>
#include <linux/irq.h>
#include <linux/jiffies.h>
#include <linux/miscdevice.h>
#include <linux/module.h>
#include <linux/msm-bus.h>
#include <linux/mutex.h>
#include <linux/of.h>
#include <linux/of_gpio.h>
#include <linux/of_platform.h>
#include <linux/platform_device.h>
#include <linux/refcount.h>
#include <linux/regmap.h>
#include <linux/regulator/consumer.h>
#include <linux/sched.h>
#include <linux/slab.h>
#include <linux/spinlock.h>
#include <linux/spmi.h>
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
