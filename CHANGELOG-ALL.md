Cl:
- kernelsu: Add KernelSU (backslashxx fork, KSU_VERSION 32657)
- nomount: Add NoMount support
- misc: Add ntsync (NT synchronization primitives)
- fs: Add compat_ptr_ioctl() helper
- mm: Add RW_SWAPPINESS to allow runtime vm.swappiness change
- binder: Add frozen notification
- binder: Use bitmap for faster descriptor lookup
- binder: Fix descriptor lookup for context manager
- binderfs: Drop duplicated struct binder_features
- workqueue: Reduce expensive locks for unbound workqueue
- rmnet: Restore skb->dev on deaggregated frames
- rmnet: Fix endpoint use-after-free in rmnet_dellink()
- rmnet: Validate MAP frame length before ingress parsing
- netfilter: xt_quota2: Fix UAF in q2_get_counter error path
- klist: Avoid accesses after waking klist_remove()
- mm: mmap: Fix fput in error path
- mm: Introduce vma_set_file()
- mm/mremap: Always take rmap locks in the optimized HPAGE_PMD move
- fs-writeback: Switch to power efficient workqueue for dirty time updates
- smp: Reduce preemption disabled sections in smp_call_function*()
- tick/nohz: Avoid unused timekeeping_max_deferment() calls
- tick/nohz: Remove redundant IRQ save/restore
- taskstats: Copy signal->stats under siglock in taskstats_exit
- lib/lzo: Add lzo-rle support
- lib/lzo: Fast 8-byte copy on arm64
- lib/lzo: 64-bit CTZ on arm64
- lib/lzo: Fix alignment bug in lzo-rle
- lib/lzo: Fix ambiguous encoding bug in lzo-rle
- lib/lzo: Fix bugs for very short or empty input
- lib/lzo: Tidy-up ifdefs
- lockdep: Add lockdep_assert() family and LOCK_STATE_* defines
- kallsyms: Un-gate kallsyms_on_each_symbol from CONFIG_LIVEPATCH
- venus: Resolve committed merge-conflict markers in hfi_venus.c
- lib: test_overflow: Resolve committed merge-conflict markers
- arch: arm64: configs: Enable NTSYNC, RW_SWAPPINESS, LZO (F2FS LZO/LZO-RLE)

Sumber: ~/kernel_sm8250 (F3) + ~/android_kernel_samsung_sm8250 (LineageOS)
Branch: port/tier-abc + port/ss-picks (di atas `back` @ 1bd40e9cae24)
Catatan: seri genirq/cpuhotplug (6 commit) sudah di-revert — efeknya nol, tidak didaftar.
