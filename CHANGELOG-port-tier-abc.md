# Changelog — whyred 4.19 port Tier A + Tier C

**Branch:** `port/tier-abc`
**Base:** `back` @ `1bd40e9cae24` (tag `backup/pre-port-20261005-180946`)
**Head:** `b19cc5e38e9f`
**Tanggal:** 2026-10-05
**Sumber fitur:** `~/kernel_sm8250` (F3, Linux 4.19.337, fork `re-noroi`)
**Target:** `~/android_kernel_xiaomi_sdm660_419` (Linux 4.19.325, multidevice sdm660/636)

**Build:** sukses — `Linux 4.19.325-st20-Maya-Kernel_Sienna`, clang r547379, LTO thin
**DTB:** `sdm636-mtp-whyred.dtb`
**Artefak:** `kernel-4.19-port-tier-abc-06102026-0243.zip` (13.9 MB, AnyKernel3)
**MD5:** `efa63ef2500e8701ce23f07e85ff8133`

Total: **27 commit** di atas checkpoint.

---

## Tier A — fitur baru (self-contained)

### A1. ntsync — NT synchronization primitives
`731221b5a373` — Port dari F3 (`dc1defa88f94` / maxsteeel ntsync).
- `drivers/misc/ntsync.c` (+1224), `include/uapi/linux/ntsync.h` (+59)
- `compat_ptr_ioctl()` di `fs/ioctl.c` + deklarasi di `include/linux/fs.h`
- `drivers/misc/Kconfig` + `Makefile`, `CONFIG_NTSYNC=y` (whyred defconfig)
- Kegunaan: emulasi primitive sinkronisasi Windows NT untuk Wine/Proton/FEX

### A2. RW_SWAPPINESS — vm.swappiness bisa diubah runtime
`a314750b58fc` — Port dari F3 (`dc1defa88f94`).
- `CONFIG_RW_SWAPPINESS` (`mm/Kconfig`) menggerbang mode sysctl antara 0644 (rw) / 0444 (ro)
- `kernel/sysctl.c`, `CONFIG_RW_SWAPPINESS=y` (whyred defconfig)

### A3. Binder frozen notification
`96359dd58156` — Port dari F3 (`bc4cf602ae66`). **Port manual penuh** (binder N5 beda generasi).
- uapi: `struct binder_frozen_state_info`; `BR_TRANSACTION_PENDING_FROZEN`,
  `BR_FROZEN_BINDER`, `BR_CLEAR_FREEZE_NOTIFICATION_DONE`;
  `BC_REQUEST_FREEZE_NOTIFICATION`, `BC_CLEAR_FREEZE_NOTIFICATION`,
  `BC_FREEZE_NOTIFICATION_DONE`
- `binder_internal.h`: `struct binder_ref_freeze`, `ref->freeze`,
  `proc->delivered_freeze`, `BINDER_STAT_FREEZE`, work type baru, resize array stats
- `binder.c`: `binder_request_freeze_notification`,
  `binder_clear_freeze_notification`, `binder_freeze_notification_done`,
  `binder_add_freeze_work`; flag `frozen` di `binder_proc_transaction` +
  `BR_TRANSACTION_PENDING_FROZEN`; case di `binder_thread_write`/`read`;
  case di `binder_release_work`; string tabel stats
- `binderfs.c`: node `features/freeze_notification`
- Adaptasi: N5 belum pakai `kmem_cache` untuk ref → `kzalloc`/`kfree`
  (bukan `binder_ref_freeze_pool`)

---

## Tier C — fix generik (stabilitas / security)

### Subsystem: net (Qualcomm rmnet)
- `31d8cc5424c4` — rmnet: restore `skb->dev` on deaggregated frames (`85cb2f6dc6e1`)
- `96ce2cb76397` — rmnet: fix endpoint use-after-free in `rmnet_dellink()` (`3465f071284c`)
- `c931e48a2c32` — rmnet: validate MAP frame length before ingress parsing (`3d86f7a5f98a`)

### Subsystem: netfilter
- `44662beb4c5f` — **ANDROID: xt_quota2: fix UAF in `q2_get_counter` error path** (`b4f98275d08c`)

### Subsystem: workqueue / scheduler / time
- `0fa1eece4547` — workqueue: reduce expensive locks for unbound workqueue (`0719b56d85db`)
- `f098d7567eab` — smp: reduce preemption-disabled sections in `smp_call_function*()` (`d94e200b062c`)
- `57db1b23b26e` — tick/nohz: avoid unused `timekeeping_max_deferment()` calls (`1f354ed74e76`)
- `3349cb34d5ac` — tick/nohz: remove redundant IRQ save/restore (`3e81a392a864`)
- `7aa0d2c77737` — fs-writeback: power-efficient workqueue untuk dirty-time (`eedcc6b1609c`)

### Subsystem: genirq / CPU hotplug (series 6 commit)
- `af1560323f7a` — skip suspended interrupts when restoring affinity (`0c7759961733`)
- `f07102ced0ef` — notify about affinity changes breaking the affinity mask (`a83e512f099b`)
- `6a441485ce46` — retry with `cpu_online_mask` when migration fails (`6104377d7ca3`)
- `012c28c5ecc4` — retain disable depth for managed interrupts across CPU hotplug (`6c0e3798077d`)
- `b1ab026e9623` — rebalance managed interrupts across multi-CPU hotplug (`ead6a316176e`)
- `f0d11fe503f1` — restore affinity even for suspended IRQ (`5b2465203672`)

### Subsystem: mm / fs / misc
- `f2ece406914d` — mm: mmap: fix fput in error path v2 (`5e089e08283a`)
- `3637f436ab26` — mm: introduce `vma_set_file()` v5 (`205f33b362d1`)
- `fd268a857ae8` — klist: avoid accesses after waking `klist_remove()` (`b86abada5642`)
- `bb3dc21247b6` — taskstats: copy `signal->stats` under siglock in `taskstats_exit` (`c6a5d897b810`)

---

## Fix wajib agar build lolos (bukan Tier A–C)

- `b55a5426bb39` — **lockdep: tambah `lockdep_assert()` + `LOCK_STATE_*`** —
  dibutuhkan ntsync; `lockdep_assert_held_exclusive` dipertahankan sebagai alias
  (sumber: F3 `dc1defa88f94` / `1b251ed3225ed`)
- `b19cc5e38e9f` — **kallsyms: un-gate `kallsyms_on_each_symbol` dari `CONFIG_LIVEPATCH`** —
  dipanggil KSU, di 4.19 hanya terdefinisi kalau LIVEPATCH=y → `LD vmlinux` gagal
- `16723ba00332` — **binderfs: hapus `struct binder_features` duplikat** —
  N5 sudah punya deklarasi itu di atas file → `redefinition`
- `0ffa7bd4e931` — **venus: resolve conflict marker di `hfi_venus.c`** —
  marker `275f39bfdbcc` nyangkut ke-commit di tree, bikin build gagal
- `5e6aaefccf7d` — **lib/test_overflow: resolve conflict marker** —
  marker `f85a226d2aa2` nyangkut ke-commit di tree

---

## TIDAK termasuk (Tier B / di-skip)

**Tier B — zram modern backend + lz4 NEON + zstd: di-skip.**
Alasan: bukan cherry-pick, tapi **cascade backport** —
- `zs_obj_read_begin/end`, `zs_obj_write`, `zs_lookup_class_index`, `zs_huge_class_size`
  → butuh zsmalloc 6.x (handle-class encoding); N5 masih API lama
- `ZRAM_BACKEND_*` butuh `LZ4_COMPRESS`/`LZ4HC_COMPRESS`/`LZ4_DECOMPRESS`
  → butuh lib/lz4 versi split; N5 belum punya `LZ4_COMPRESS` simbol
- zram 6.x sendiri (`zram_drv.c` 2882 baris + backend_*.c + zcomp)
Risiko boot-critical (zram = swap Android). Perlu branch terpisah + build+flash test.

**Di-skip karena butuh prasyarat yang tidak ada di N5:**
- workqueue `unbind_workers()` vs `wq_worker_running()` race + `wq_worker_sleeping()` warning
  → butuh `worker->sleeping` (N5 belum ada)
- zsmalloc: drop `pool->lock` dari `zs_free` (64-bit) → butuh handle-class encoding
- binder freeze-API refactor (181 baris, `binder_set_proc_frozen`/`binder_freeze_fastpath`)
  → menyasar base binder F3 yang lebih baru

**Sudah ada di N5 (no-op, tidak di-commit):**
- net: `__in6_dev_stats_get()` NULL deref, af_key IPComp, net/sched pedit COW,
  timestamp cmsgs, mld RCU
- super emergency thaw deadlock, proc `setattr()`, cgroup `cgroup_destroy_wq` split

---

## Verifikasi (`llvm-nm out/vmlinux`)

- `ntsync` — 15 simbol + `__initcall_ntsync_misc_init6` ✓
- `binder_add_freeze_work`, `BR_*`/`BC_*` freeze ✓
- `vma_set_file` ✓
- `kallsyms_on_each_symbol` ✓
- `rmnet_map_validate_packet_len` ✓
- `ksu_*` — 196 simbol ✓

---

## Rollback

```sh
git checkout back                 # kembali ke titik awal
git branch -D port/tier-abc       # hapus branch kerja
# artefak: /root/kernel-build/result/kernel-4.19-port-tier-abc-06102026-0243.zip
```
