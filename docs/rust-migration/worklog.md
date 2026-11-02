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
