# Lingkungan build (Fase 0 AGENTS_4)

Tanggal pengecekan: 2026-11-03 (WIB).
Branch: `rust-migration/fase0-baseline`, dari `sienna` (`09ce1facd3f1`).
Perhatian: angka di bawah adalah hasil pengecekan di mesin ini,
bukan salinan dari AGENTS_4. AGENTS_4 menyebut host Ubuntu VMware
dengan clang 18.1.3 dan "belum pernah dibangun" — itu TIDAK
berlaku di sini, lihat koreksi di bawah.

## Host

- `uname -a`: Linux htch-runtime 7.0.0-39-generic x86_64 GNU/Linux
- CPU: 4 core (`nproc`), RAM 7.7 GiB, disk 150G (143G tersedia)
- Python: 3.12.3
- Host gcc: Ubuntu 13.3.0
- Host tools: `bc`, `bison`, `flex` tersedia di `/usr/bin`.
  `dtc` tidak ditemukan di PATH pada pengecekan ini (tree memakai
  `scripts/dtc` internal; catat bila build mengeluh soal dtc).

## Toolchain yang dipakai untuk build baseline

- NDK r27d, clang/lld 18.0.4:
  `~/workspace/android-ndk/android-ndk-r27d/toolchains/llvm/prebuilt/linux-x86_64/bin/`
- Perintah build yang dipakai sebelumnya di mesin ini (dan dipakai
  `tools/rust-migration/build.sh`):
  `make O=<out> ARCH=arm64 LLVM=1 CROSS_COMPILE=aarch64-linux-gnu-`
  dengan PATH NDK di depan.
- Rust (TIDAK dipakai untuk baseline C ini, dicatat saja karena
  ada di mesin): rustc/cargo 1.99.1 di `~/.cargo/bin`
  (tidak ada di PATH default, harus ditambahkan manual).
  Bindgen 0.73.2 pernah terpasang untuk pekerjaan rust-backport;
  tidak relevan untuk Fase 0 AGENTS_4.

## Koreksi terhadap asumsi AGENTS_4

1. Host BUKAN Ubuntu VMware dengan clang 18.1.3. Host di sini
   adalah VM Muse dengan NDK r27d clang 18.0.4.
2. Build BUKAN pertama kali. `out/` di repo ini sudah berisi
   artefak build `rust-backport` sebelumnya (`CONFIG_RUST=y`,
   `CONFIG_KSU=y`, `CONFIG_NOMOUNT=y`). Karena itu baseline
   Fase 0 ini memakai direktori BARU `out-fase0`, bukan `out/`,
   supaya tidak tercemar artefak branch lain. Penyimpangan
   ini dicatat di sini, bukan disembunyikan.
3. Submodule di working tree TIDAK cocok dengan pointer
   superproject `sienna` (lihat `inventory.md`):
   - Pointer `sienna`: KernelSU `6658ab59`, NoMount `c7f63e3f`.
   - Working tree di disk: KernelSU v3.3.1+36 (KSU_VERSION
     32716), NoMount v22. Keduanya adalah hasil update di
     branch `rust-backport`, bukan isi `sienna` asli.
   - Akibatnya baseline dari branch ini BUKAN baseline
     `sienna` murni. Build bisa gagal atau konfigurasinya
     berubah (khususnya SUSFS, lihat `baseline.md` nanti).
   - Jangan menjalankan `git submodule update` untuk
     "memperbaiki" ini tanpa persetujuan: pointer lama
     KernelSU `6658ab59` sebelumnya tidak ditemukan di
     history backslashxx, jadi update itu kemungkinan besar
     gagal atau mengubah isi working tree yang sedang dipakai
     branch lain.

## Remote dan versi

- `make kernelversion`: 4.19.325
- Remote `origin`:
  `https://github.com/anythinganywantanywere/android_kernel_xiaomi_sdm660_419/`
- HEAD `sienna`: `09ce1facd3f1c06dd8446c71b17ea27f92bff41b`
  "fs: integrate SUSFS v2.3.0 for 4.19 non-GKI + KernelSU v3.3.1"
- Defconfig: `arch/arm64/configs/vendor/whyred-perf_defconfig`
