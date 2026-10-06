Cl:
- kallsyms: Un-gate kallsyms_on_each_symbol from CONFIG_LIVEPATCH
- lockdep: Add lockdep_assert() family and LOCK_STATE_* defines
- binderfs: Drop duplicated struct binder_features
- lib: test_overflow: Resolve committed merge-conflict markers
- venus: Resolve committed merge-conflict markers in hfi_venus.c
- taskstats: Copy signal->stats under siglock in taskstats_exit
- tick/nohz: Remove redundant IRQ save/restore
- tick/nohz: Avoid unused timekeeping_max_deferment() calls
- smp: Reduce preemption disabled sections in smp_call_function*()
- fs-writeback: Switch to power efficient workqueue for dirty time updates
- mm: Introduce vma_set_file()
- mm: mmap: Fix fput in error path
- rmnet: Validate MAP frame length before ingress parsing
- klist: Avoid accesses after waking klist_remove()
- netfilter: xt_quota2: Fix UAF in q2_get_counter error path
- rmnet: Fix endpoint use-after-free in rmnet_dellink()
- rmnet: Restore skb->dev on deaggregated frames
- workqueue: Reduce expensive locks for unbound workqueue
- binder: Add frozen notification
- mm: Add RW_SWAPPINESS to allow runtime vm.swappiness change
- misc: Add ntsync (NT synchronization primitives)
- fs: Add compat_ptr_ioctl() helper
- nomount: Add NoMount support
- kernelsu: Add KernelSU (backslashxx fork, KSU_VERSION 32657)
- arch: arm64: configs: Enable NTSYNC, RW_SWAPPINESS

Note: genirq/cpuhotplug managed-IRQ series was dropped (reverted) — verified not
the cause of the pre-existing irq_startup warning.
