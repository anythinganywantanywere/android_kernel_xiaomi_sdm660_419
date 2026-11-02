# Worklog AGENTS_4

## 2026-11-03 sesi 1
- Fase: 0 (baseline dan inventaris)
- Dikerjakan: branch `rust-migration/fase0-baseline` dari `sienna`;
  `docs/rust-migration/env.md` (toolchain NDK r27d clang 18.0.4,
  koreksi asumsi Ubuntu VMware AGENTS_4); `tools/rust-migration/build.sh`
  (default `out-fase0`, bukan `out/` lama) dan `config-check.sh`;
  `.gitignore` ditambah `out-fase0/`, `out-debug/`,
  `docs/rust-migration/logs/`; `docs/rust-migration/inventory.md`
  pra-build (71.012 file, lokasi KSU/NoMount/SUSFS, peringatan
  mismatch submodule).
- Commit/branch: belum ada commit di branch ini pada saat
  worklog ini ditulis; file di atas masih uncommitted.
- Hasil verifikasi: `make kernelversion` = 4.19.325; pointer
  submodule vs working tree diverifikasi tidak cocok (lihat
  inventory.md); build baseline belum selesai.
- Menunggu manusia: keputusan AGENTS_4 menggantikan vs paralel
  vs referensi (sesi sebelumnya) — sesi ini berjalan dengan
  asumsi paralel (branch baru, `rust-backport` tidak disentuh);
  manusia bisa mengoreksi asumsi ini. Juga menunggu hasil
  build baseline untuk `baseline.md`.

## 2026-11-03 sesi 1 (lanjutan)
- Fase: 0 (baseline dan inventaris)
- Dikerjakan: build baseline `tools/rust-migration/build.sh`
  ke `out-fase0`; `baseline.md`, `baseline.config`,
  `warnings-baseline.txt` (5 baris ternormalisasi),
  `enabled-config.txt` (1.523 baris).
- Commit/branch: lihat commit Fase 0 di branch ini.
- Hasil verifikasi: build exit 0, errors 0, warnings 7 baris;
  `Image.gz-dtb` 13.025.530 byte sha256 `62c0ec91...`;
  `vmlinux` 39.649.512 byte; `System.map` 3.687.677 byte.
  Temuan: `CONFIG_KSU_SUSFS=y` di defconfig hilang di
  `.config` akhir, `susfs.o` tidak dibangun — baseline ini
  KSU + NoMount tanpa SUSFS.
- Menunggu manusia: keputusan AGENTS_4 (menggantikan vs
  paralel vs referensi) masih terbuka; sesi ini memakai
  asumsi paralel. Juga keputusan soal SUSFS off ini
  (biarkan, atau perbaiki di fase berikutnya).

## 2026-11-03 sesi 2
- Fase: 1 (hardening konfigurasi)
- Dikerjakan: cek 14 kandidat AGENTS_4; aktifkan 3 opsi aman
  (HARDENED_USERCOPY, SLAB_FREELIST_RANDOM,
  SLAB_FREELIST_HARDENED) di
  `arch/arm64/configs/vendor/whyred-perf_defconfig` (+3 baris);
  CFI/SCS sengaja tidak diaktifkan (menunggu keputusan);
  koreksi prosedur savedefconfig (tidak dipakai menggantikan
  defconfig vendor, karena akan membuang blok KSU_SUSFS dll.);
  build penuh ke `out-fase1` dengan 2 interupsi yang dicatat
  (restart runtime, lalu bc/bison/flex hilang dan di-install
  ulang).
- Commit/branch: lihat commit Fase 1 di branch ini.
- Hasil verifikasi: build selesai 05:38 WIB, errors 0,
  warnings 7 baris, tidak ada warning baru vs baseline;
  Image.gz-dtb 13.037.193 byte sha256 `5beacb20...`;
  `.config` akhir memuat ketiga opsi =y, KSU=y, NOMOUNT=y,
  tanpa KSU_SUSFS (sama seperti baseline).
- Menunggu manusia: keputusan CFI/SCS, keputusan SUSFS off,
  dan uji boot manual image Fase 1 (belum ada flash).
