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

## 2026-11-03 sesi 3
- Fase: 2 (sinkronisasi upstream)
- Dikerjakan: cek kernelversion dan tag stable terbaru lewat ls-remote; tulis upstream-sync.md
- Commit/branch: bd8a5573 di rust-migration/fase0-baseline
- Hasil verifikasi: make kernelversion -> 4.19.325; tag stable terbaru -> v4.19.325; jarak 0 rilis, tidak ada batch merge; tidak ada tag CAF di repo lokal
- Menunggu manusia: tidak ada dari Fase 2; perbaikan masa depan lewat cherry-pick bulletin Android

## 2026-11-03 sesi 4
- Fase: 3 (audit hotspot)
- Dikerjakan: grep terarah + LOC + bukti objek terbangun untuk kandidat §9.2; tulis hotspots.md dan bugs/BUG-001.md + bugs/README.md
- Commit/branch: 39529eb5 di rust-migration/fase0-baseline
- Hasil verifikasi: skor tertinggi WLAN 9, Bluetooth 8, audio/kamera/char 7; sparse/spatch tidak tersedia (dicatat, tidak diklaim); satu kandidat terverifikasi BUG-001 (strcpy EEPROM kamera legacy, hanya lewat device tree); sprintf sensor fusion dan rmtfs_mem diperiksa dan bukan bug
- Menunggu manusia: tidak ada dari Fase 3; penelaahan manual driver skor tinggi adalah pekerjaan lanjutan, bukan klaim selesai

## 2026-11-03 sesi 5
- Fase: 4 (perbaikan C + Rust userspace)
- Dikerjakan: patch minimal BUG-001 (strcpy -> strscpy, setelah strlcpy ditolak checkpatch dengan prefer strscpy); build incremental; workspace whyred-rust-tools (kapi + kernel-info) dengan fmt/clippy/test/cross-build
- Commit/branch: c4744b6f di rust-migration/fase0-baseline (patch C); 9265ee4 di repo git lokal whyred-rust-tools (Rust userspace, tanpa remote/push)
- Hasil verifikasi: checkpatch 0 errors 0 warnings; build incremental out-fase1: msm_eeprom.o dan Image.gz-dtb rebuilt 16:14 WIB, sha256 Image baru c363f88e..., errors 0; Rust: fmt clean, clippy -D warnings clean, test 4/4 lulus, cross-build aarch64-linux-android release sukses (ELF aarch64 274K, /system/bin/linker64); host run kernel-info sukses (output kernel host, bukan whyred)
- Menunggu manusia: uji boot image patch BUG-001; eksekusi kernel-info di perangkat; konfirmasi lokasi repo userspace (terpisah vs userspace-rs/)

## 2026-11-03 sesi 6
- Fase: 5 (kelayakan kernel 6.x, dokumen saja)
- Dikerjakan: riset sumber (sdm660-mainline, remoteproc 6.6, fuel gauge lavender, postmarketOS whyred, Adreno 5xx, Rust 6.1, patch SPI/I2C); tulis feasibility-6x.md
- Commit/branch: 4749a417 di rust-migration/fase0-baseline
- Hasil verifikasi: sumber dicek 2026-11-03 dan dikutip di dokumen; koreksi: whyred (SDM636) memakai Adreno 509, bukan 508; tidak ada driver vendor yang dipindahkan ke Rust; tidak ada perubahan tree 4.19 dari dokumen ini
- Menunggu manusia: keputusan lanjut/tunda/batal untuk riset kernel 6.x (rekomendasi dokumen: tunda untuk Android, lanjut riset Linux non-Android saja)

## 2026-11-03 sesi 7
- Fase: uji manual perangkat (§12, untuk Fase 1 dan Fase 4)
- Dikerjakan: tulis device-test-2026-11-03.md dari log manusia (Termux); tidak ada perubahan kode
- Commit/branch: lihat commit uji perangkat di branch ini
- Hasil verifikasi: uname/version cocok dengan build patch 16:14 WIB; dmesg tanpa BUG/Oops/panic, 2 WARNING tercatat (place_entity, irq_startup, penyebab vs baseline belum dipastikan); getenforce Enforcing; kernel-info cocok dengan uname, lengkap di bawah su; manusia menyatakan perangkat aman 16:46 WIB
- Menunggu manusia: perbandingan dmesg baseline untuk 2 WARNING; laporan rinci reboot 3x dan perangkat keras bila ada regresi; keputusan CFI/SCS, SUSFS, lokasi userspace, dan push (di luar penutupan tugas AGENTS_4 ini)
