/* SPDX-License-Identifier: GPL-2.0 */
/*
 * Reflex CPUFreq governor: the only kernel-version differences.
 * cpufreq_reflex.c is byte-identical across versions; this file is not.
 *
 * Linux 4.19 port:
 *  - no cpufreq_driver_adjust_perf()/..._has_adjust_perf(); the perf path is
 *    unused.  rfx_update_single_perf() is compiled out below, so the fast
 *    path goes through cpufreq_driver_fast_switch() exactly as schedutil.
 *  - no cpufreq_driver_test_flags() and no CPUFREQ_NEED_UPDATE_LIMITS; the
 *    flag is never set on 4.19, so the test reports "not set".
 *  - no .flags in struct cpufreq_governor; use .dynamic_switching.
 *  - Energy Model uses struct em_cap_state/{frequency,power,cost} in
 *    pd->table and has no RCU table swap, so there is no "em changed".
 */
#ifndef _CPUFREQ_REFLEX_COMPAT_H
#define _CPUFREQ_REFLEX_COMPAT_H

#include <linux/cpufreq.h>

/* 4.19: external governors cannot use the adjust_perf path at all. */
#define RFX_NO_ADJUST_PERF 1

static inline int cpufreq_driver_has_adjust_perf(void)
{
	return 0;
}

/* Never set by any 4.19 driver. */
#ifndef CPUFREQ_NEED_UPDATE_LIMITS
#define CPUFREQ_NEED_UPDATE_LIMITS 0
#endif

/* Defined in drivers/cpufreq/cpufreq.c (approach kept from the vendor port). */
bool cpufreq_driver_test_flags(unsigned int flags);

#endif /* _CPUFREQ_REFLEX_COMPAT_H */