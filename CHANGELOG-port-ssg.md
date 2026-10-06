Cl:
- block: Import SSG (Samsung Generic) I/O scheduler
  (import G998USQU5CVDB + update S908BXXU2AVF1, backported to 4.19)
- block: Add blkcg shallow-depth control for SSG (ssg.max_available_ratio)
- block: Make SSG the default blk-mq I/O scheduler (MQ_SSG_DEFAULT)
- arch: arm64: configs: Enable SSG + SSG cgroup, set zram default compressor to lz4
- soc: qcom: Stop MSM_PM from force-selecting MSM_IDLE_STATS

Sumber: ~/anya_xiaomi_sm6115 (sm6115/bengal, 4.19.325)
Commit sumber:
- 20975f5b  block: import ssg from G998USQU5CVDB
- 3f45ea51  block: update ssg from S908BXXU2AVF1
- 770a89b2  BACKPORT: backport ssg scheduler to 4.19
- 71352274  zram: set default compressor to lz4

Branch: sienna (di atas 9f444e1362c4)
Commit hasil: 745819666e08 (SSG, ssg 3 commit → squashed), 1181f46f4f2f (zram),
c6ed10e29ee4 (MSM_PM/IDLE_STATS), 46df2bfdc603 (revert DEBUG_FS).

Penyesuaian yang diperlukan (kenapa tidak bisa cherry-pick mentah):
- ssg_completed_request() dipertahankan 2-argumen (struct request *rq, u64 now):
  elevator_mq_ops.completed_request di tree ini membawa u64, beda dari sumber.
- block/Makefile di-merge 3-way: tree ini sudah membuang sched single-queue
  legacy (deadline/cfq), konteks hunk sumber tidak cocok.
- MQ_IOSCHED_SSG_CGROUP wajib =y: ssg-iosched.c memanggil blkcg_css() yang hanya
  didefinisikan di bawah #if CONFIG_MQ_IOSCHED_SSG_CGROUP. Tanpa itu build gagal
  (-Werror=implicit-function-declaration).
- zram: patch file vendor (zram_drv.c / zram/Kconfig) tidak dipakai karena tree
  ini memakai implementasi zram baru (choice CONFIG_ZRAM_DEF_COMP_*); cukup
  satu baris defconfig (CONFIG_ZRAM_DEF_COMP_LZ4=y).

Dropped / di-revert:
- CONFIG_DEBUG_FS + CONFIG_BLK_DEBUG_FS (untuk atribut debugfs SSG): bootloop di
  whyred — hang di logo Mi, tanpa log (pstore kosong; DTS whyred tidak punya
  node ramoops). Di-revert di 46df2bfdc603. Efek: atribut debugfs SSG
  (read/write_fifo_list, *_next_rq, starved_writes, dispatch) tidak tersedia.
- blk_sec_stats accounting (bagian update S908BXXU2AVF1): tree ini tidak punya
  simbol CONFIG_BLK_SEC_STATS, jadi ter-compile sebagai no-op.

Verifikasi on-device (whyred, serial 13a36a93):
- /sys/block/mmcblk0/queue/scheduler = "kyber [ssg] bfq none" (ssg aktif/default)
- /sys/block/mmcblk0/queue/iosched/: tgroup_shallow_depth=4,
  async_write_shallow_depth=2, read_expire=500, write_expire=5000,
  max_write_starvation=2, front_merges=1
- /dev/blkio/background/blkio.ssg.max_available_ratio = 100 (bisa ditulis)
- /sys/block/zram0/comp_algorithm = "[lz4] "
- /proc/kallsyms: 45+ simbol ssg_* (ssg_iosched_init, ssg_blkcg_init, dst)
