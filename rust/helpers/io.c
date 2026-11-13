// SPDX-License-Identifier: GPL-2.0

#include <linux/io.h>

/*
 * `ioremap`, `iounmap`, `readl` and `writel` are static inlines (or
 * macros) in this tree, so `bindgen` cannot see them. These helpers
 * are the only way for Rust to call them; the helpers `bindgen` pass
 * exposes them as `bindings::ioremap` etc.
 */
void __iomem *rust_helper_ioremap(phys_addr_t offset, size_t size)
{
	return ioremap(offset, size);
}

void rust_helper_iounmap(volatile void __iomem *addr)
{
	iounmap(addr);
}

u32 rust_helper_readl(const volatile void __iomem *addr)
{
	return readl(addr);
}

void rust_helper_writel(u32 value, volatile void __iomem *addr)
{
	writel(value, addr);
}
