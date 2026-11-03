// SPDX-License-Identifier: GPL-2.0

#include <linux/slab.h>

/* __realloc_size does not exist in 4.19. */
void * __must_check
rust_helper_krealloc(const void *objp, size_t new_size, gfp_t flags)
{
	return krealloc(objp, new_size, flags);
}
