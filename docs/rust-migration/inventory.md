# Inventaris kode (Fase 0 AGENTS_4, pra-build)

Branch: `rust-migration/fase0-baseline`, dari `sienna`.
HEAD: `09ce1facd3f1c06dd8446c71b17ea27f92bff41b`
"fs: integrate SUSFS v2.3.0 for 4.19 non-GKI + KernelSU v3.3.1"
Versi: `make kernelversion` = 4.19.325.
Remote: `https://github.com/anythinganywantanywere/android_kernel_xiaomi_sdm660_419/`
Defconfig: `arch/arm64/configs/vendor/whyred-perf_defconfig`
Jumlah file terlacak: 71.012 (`git ls-files | wc -l`).

## Peringatan utama: baseline ini bukan sienna murni

Pointer superproject `sienna` vs working tree di disk:

- KernelSU: pointer `6658ab59`, working tree v3.3.1+36
  (KSU_VERSION 32716, dari pekerjaan `rust-backport`).
- NoMount: pointer `c7f63e3f` (v2.1.0), working tree v2.2.0
  (protokol v22, dari pekerjaan `rust-backport`).

Karena itu build baseline di branch ini memakai kode driver
KernelSU/NoMount BARU dengan superproject LAMA. Dampak yang
sudah bisa diprediksi sebelum build (harus dibuktikan build,
bukan diasumsikan sebagai hasil):

- `fs/Kconfig` di `sienna` mengandung 0 simbol `KSU_SUSFS`.
  Di `sienna` asli simbol itu kemungkinan didefinisikan di
  Kconfig driver KernelSU lama. Driver baru tidak punya
  simbol itu sama sekali, jadi `CONFIG_KSU_SUSFS=y` di
  defconfig kemungkinan akan dibuang saat `olddefconfig`.
- `fs/susfs.c` mendeklarasikan dan memanggil
  `susfs_is_current_ksu_domain()`, yang tidak didefinisikan
  di driver baru (padanannya di driver baru adalah
  `is_ksu_domain()` di `KernelSU/kernel/selinux/selinux.c`).
  Tanpa shim seperti yang ada di branch `rust-backport`,
  build ini berisiko gagal link. Jangan menyalin shim itu
  diam-diam ke branch ini; kalau build gagal karena ini,
  catat sebagai satu penyebab di `baseline.md` dan minta
  keputusan manusia sebelum memperbaiki.

## Lokasi KernelSU, NoMount, SUSFS (catatan saja, jangan ubah)

- `drivers/kernelsu` -> symlink ke `../KernelSU/kernel`
- `drivers/Kconfig:233` me-source `drivers/kernelsu/Kconfig`
- `drivers/Makefile:196`: `obj-$(CONFIG_KSU) += kernelsu/`
- `fs/nomount` -> symlink ke `../NoMount/kernel/src`
- SUSFS v2.3.0: `fs/susfs.c`, `include/linux/susfs.h`,
  `include/linux/susfs_def.h`, hook di `kernel/kallsyms.c`,
  `kernel/sys.c`, dan beberapa file `fs/`.
- Defconfig terkait: `CONFIG_LOCALVERSION="-Maya-Kernel_Sienna-v2.5"`,
  `CONFIG_KSU=y`, `CONFIG_KSU_SUSFS=y`, `CONFIG_NOMOUNT=y`,
  tidak ada baris `CONFIG_MODULES` (artinya modul off secara
  default untuk build perangkat).

## Direktori yang relevan untuk audit Fase 3 (verifikasi keberadaan)

- WLAN: `drivers/staging/qcacld-3.0` ADA (Kbuild/Kconfig ada).
- GPU: `drivers/gpu/msm` ADA (adreno a3xx/a5xx/a6xx).
- `drivers/soc/qcom`, `techpack/audio`, dan driver kamera/media
  perlu diverifikasi terpisah saat Fase 3; inventaris ini
  tidak mengklaim daftar lengkap vendor vs upstream hanya
  dari header lisensi, karena itu butuh pemindaian riwayat
  commit yang belum dijalankan di Fase 0 ini.
- Hitung baris kode kasar (file `.c`/`.h` saja, `wc -l`,
  termasuk baris kosong/komentar, bukan `cloc`):
  `arch/arm64` 71.680, `drivers` 1.900.777, `fs` 1.334.187,
  `kernel` 380.771, `mm` 144.301, `net` 1.058.753,
  `sound` 1.076.001, `techpack` 457.178.
  Angka ini kasar dan dipakai hanya untuk gambaran ukuran,
  bukan untuk prioritas audit (prioritas pakai skor §9
  AGENTS_4, bukan ukuran kode).

## Yang belum ada / belum diverifikasi di Fase 0 ini

- `baseline.md`, `warnings-baseline.txt`, `enabled-config.txt`:
  menunggu hasil build baseline (lihat `worklog.md`).
- Daftar driver aktif whyred dari `out-fase0/.config`:
  menunggu build.
- Pemisahan vendor vs upstream per direktori: belum dikerjakan,
  masuk backlog Fase 0/Fase 3, bukan dikarang di sini.
