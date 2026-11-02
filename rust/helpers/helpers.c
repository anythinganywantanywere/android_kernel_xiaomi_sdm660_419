// SPDX-License-Identifier: GPL-2.0
/* Only the helpers the minimal module uses. Dropped from the
 * 6.12 reference: blk, kunit, page, rbtree, signal, task,
 * uaccess, wait, workqueue, and build_bug (errname does not
 * exist in 4.19).
 */

#include "bug.c"
#include "build_assert.c"
#include "err.c"
#include "mutex.c"
#include "refcount.c"
#include "slab.c"
#include "spinlock.c"
