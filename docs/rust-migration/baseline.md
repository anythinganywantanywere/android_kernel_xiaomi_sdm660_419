# Baseline build (Fase 0 AGENTS_4)

Branch: `rust-migration/fase0-baseline`, dari `sienna` (`09ce1facd3f1`).
Defconfig: `vendor/whyred-perf_defconfig`.
Perintah: `tools/rust-migration/build.sh vendor/whyred-perf_defconfig out-fase0`
Toolchain: NDK r27d clang/lld 18.0.4, `ARCH=arm64 LLVM=1
CROSS_COMPILE=aarch64-linux-gnu-` (lihat `env.md`).
Waktu build: mulai 2026-11-03 04:48 WIB, selesai 05:09 WIB
(~21 menit, `-j4`).
Log: `docs/rust-migration/logs/` (tidak di-commit, di-ignore).

## Hasil: BERHASIL (exit 0, errors 0)

- `out-fase0/arch/arm64/boot/Image.gz-dtb`: 13.025.530 byte,
  sha256 `62c0ec9199f9a78a9353dc9bf2e3abce666192662f83a62474f5afb698d01423`
- `out-fase0/vmlinux`: 39.649.512 byte
- `out-fase0/System.map`: 3.687.677 byte
- Warnings: 7 baris log, 5 baris ternormalisasi
  (lihat `warnings-baseline.txt`):
  - 1 pre-existing: `drivers/cpufreq/cpufreq_reflex.c:489`
    unused `rfx_floor_util` (sudah ada di semua build
    sebelumnya di mesin ini).
  - 6 baris clang `constant-logical-operand` dari build
    KernelSU (header kernel/netdevice/pgtable/slab).
    Tidak ada flag `-Wno-*` yang ditambahkan.
- `out-fase0/.config` disalin ke `baseline.config` di sini.
- Driver aktif: 1.523 baris `=y/=m` di `enabled-config.txt`.

## Temuan terpenting: SUSFS hilang diam-diam

Defconfig meminta `CONFIG_KSU_SUSFS=y`, TAPI `.config` akhir
TIDAK mengandung simbol SUSFS sama sekali, dan
`out-fase0/fs/susfs.o` tidak dibangun. Yang dibangun:
`drivers/kernelsu/ksu.o` (KSU=y) dan
`fs/nomount/nomount.o` (NOMOUNT=y).

Penyebabnya konsisten dengan prediksi di `inventory.md`:
`fs/Kconfig` di `sienna` tidak mendefinisikan `KSU_SUSFS`,
dan driver KernelSU baru di working tree juga tidak punya
simbol itu, jadi `olddefconfig` membuang opsi tersebut
tanpa error. Build tidak gagal link karena `fs/susfs.c`
tidak dikompilasi sama sekali (aturan `fs/Makefile`
`obj-$(CONFIG_KSU_SUSFS) += susfs.o`).

Artinya: image baseline ini adalah KSU + NoMount,
TANPA SUSFS, meskipun defconfig-nya menulis SUSFS=y.
Jangan mengklaim baseline ini "SUSFS aktif". Perbaikan
(atau keputusan untuk membiarkan SUSFS off) adalah
pekerjaan Fase 1/keputusan manusia, bukan bagian dari
Fase 0, dan tidak boleh disalin diam-diam dari branch
`rust-backport` (di sana ada shim + Kconfig di
superproject).

## Temuan lain

- `CONFIG_RUST` tidak ada di `.config` akhir, sesuai
  strategi AGENTS_4 (kernel tetap C). Tidak ada file
  `rust/` yang dipakai build ini.
- `CONFIG_MODULES` tidak diset, sesuai defconfig
  (tidak ada barisnya di defconfig).
- `CONFIG_LOCALVERSION="-Maya-Kernel_Sienna-v2.5"`.
- Baseline ini BUKAN `sienna` murni (lihat `inventory.md`):
  superproject `sienna`, working tree KernelSU v3.3.1+36
  dan NoMount v2.2.0. Angka di atas berlaku untuk
  kombinasi itu, bukan untuk `sienna` dengan driver lama
  yang sumbernya tidak tersedia di mesin ini.

## Yang belum diverifikasi

- Boot di perangkat: TIDAK. Agent tidak menjalankan
  `fastboot`/`adb flash` apa pun. Image ini belum diuji
  boot, dan tidak boleh diklaim sudah boot sebelum
  manusia melapor (daftar uji §12 AGENTS_4).
- Tidak ada zip TWRP yang dibuat dari image ini di Fase 0.
