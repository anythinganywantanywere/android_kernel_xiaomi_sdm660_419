// SPDX-License-Identifier: GPL-2.0
/* Only the helpers the minimal module uses. Dropped from the
 * 6.12 reference: blk, kunit, page, rbtree, signal, task,
 * wait, workqueue, and build_bug (errname does not
 * exist in 4.19).
 *
 * Added for the file operations abstraction: uaccess
 * (`copy_from_user` / `copy_to_user` wrappers).
 *
 * Added for the memory-mapped I/O abstraction: io
 * (`ioremap` / `iounmap` / `readl` / `writel` wrappers).
 *
 * Added for the interrupt abstraction (Step 10 cleanup, item 2):
 * irqflags (`local_irq_save` / `local_irq_restore` wrappers).
 * `generic_handle_irq` expects interrupts disabled; the proof
 * driver's `fire` runs in process context where they are on,
 * which the core answers with a WARN_ONCE. Saving and restoring
 * the flags around the call removes it.
 */

#include "bug.c"
#include "build_assert.c"
#include "err.c"
#include "mutex.c"
#include "refcount.c"
#include "slab.c"
#include "spinlock.c"
#include "uaccess.c"
#include "io.c"
#include "irqflags.c"
