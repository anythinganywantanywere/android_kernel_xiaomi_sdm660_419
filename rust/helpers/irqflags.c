// SPDX-License-Identifier: GPL-2.0

#include <linux/irqflags.h>

/*
 * `local_irq_save` and `local_irq_restore` are macros / static
 * inlines in this tree, so `bindgen` cannot see them. These
 * helpers are the only way for Rust to call them; the helpers
 * `bindgen` pass exposes them as `bindings::local_irq_save`
 * etc. (`local_irq_save` saves the current interrupt state and
 * disables interrupts on this CPU; `local_irq_restore` puts it
 * back. The pair is nesting-safe, unlike a bare
 * `local_irq_disable` / `local_irq_enable`.)
 */
unsigned long rust_helper_local_irq_save(void)
{
	unsigned long flags;

	local_irq_save(flags);
	return flags;
}

void rust_helper_local_irq_restore(unsigned long flags)
{
	local_irq_restore(flags);
}
