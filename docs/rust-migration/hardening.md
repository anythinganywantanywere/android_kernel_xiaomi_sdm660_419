# Status opsi hardening (Fase 1 AGENTS_4)

Branch: `rust-migration/fase0-baseline`.
Baseline pembanding: `docs/rust-migration/baseline.config`
(build Fase 0, `out-fase0`, Image.gz-dtb sha256 `62c0ec91...`).
Build Fase 1: `out-fase1`, masih berjalan saat dokumen ini
pertama ditulis; hasil build ditambahkan di bawah setelah
selesai. Jangan mengklaim build Fase 1 lulus sebelum
bagian itu terisi.

## Metode (§7.1 AGENTS_4, dengan satu koreksi penting)

1. Status awal dibaca dari `baseline.config`, keberadaan
   di Kconfig dicek dengan `git grep` per simbol.
2. Tiga opsi diubah lewat `scripts/config` di `out-fase1`,
   lalu `olddefconfig`, lalu diverifikasi benar-benar `=y`
   di `.config` (bukan hanya diasumsikan).
3. KOREKSI terhadap langkah `savedefconfig` di AGENTS_4:
   `savedefconfig` TIDAK dipakai untuk menggantikan
   defconfig vendor. Diff `savedefconfig` vs defconfig
   vendor sangat besar dan menyesatkan: ia membuang opsi
   vendor yang tidak dikenal Kconfig (misalnya seluruh
   blok `KSU_SUSFS`, `POLLY_CLANG`, dan lain-lain) dan
   menyusun ulang urutan. Menggantikannya akan merusak
   defconfig. Sesuai prinsip AGENTS_4 sendiri ("salin
   hanya selisih yang dimaksud"), hanya 3 baris yang
   dimaksud yang ditambahkan manual ke
   `arch/arm64/configs/vendor/whyred-perf_defconfig`,
   lalu dibuktikan bahwa defconfig itu saja (tanpa
   `scripts/config`) menghasilkan ketiga opsi `=y` di
   `.config` baru. Bukti itu ada di bawah.
4. Opsi berisiko tinggi TIDAK diaktifkan di Fase 1 ini
   (lihat tabel). AGENTS_4 §3.2/§7.2 mewajibkan bertanya
   dulu untuk CFI, Shadow Call Stack, dan PANIC_ON_OOPS.
   Instruksi manusia terakhir ("lanjutkan sesuai tugasmu")
   tidak menjawab pertanyaan itu, jadi opsi-opsi itu
   dibiarkan apa adanya dan dicatat sebagai keputusan
   tertunda, bukan diaktifkan diam-diam.

## Perubahan yang dilakukan (satu kelompok, 3 opsi)

File: `arch/arm64/configs/vendor/whyred-perf_defconfig`
(+3 baris, 2 hunk):

- Setelah `CONFIG_SLUB_CPU_PARTIAL=y`:
  `CONFIG_SLAB_FREELIST_RANDOM=y`
  `CONFIG_SLAB_FREELIST_HARDENED=y`
- Sebelum `CONFIG_SECURITY_SELINUX=y`:
  `CONFIG_HARDENED_USERCOPY=y`

Efek samping yang terverifikasi di `.config`:
`CONFIG_HARDENED_USERCOPY_FALLBACK=y` muncul otomatis
(default y di Kconfig saat `HARDENED_USERCOPY=y`).
`CONFIG_STRICT_DEVMEM` tidak muncul di `.config` sama
sekali, jadi tidak ada perubahan tambahan dari `imply`
di sini.

## Tabel status

| Opsi | Status awal | Status akhir | Catatan |
|---|---|---|---|
| RANDOMIZE_BASE | y | y (tidak diubah) | Sudah aktif di baseline |
| STACKPROTECTOR_STRONG (nama 4.19; AGENTS_4 menulis CC_STACKPROTECTOR_STRONG) | y | y (tidak diubah) | Nama AGENTS_4 tidak ada di Kconfig 4.19 ini; yang ada `CONFIG_STACKPROTECTOR_STRONG=y` dan sudah aktif |
| HARDENED_USERCOPY | is not set | y | Diaktifkan Fase 1; depends `HAVE_HARDENED_USERCOPY_ALLOCATOR=y` terpenuhi; risiko: bisa panic pada driver yang salah (catat untuk uji manual) |
| FORTIFY_SOURCE | tidak ada di .config | tidak diubah | Ada di `security/Kconfig`, tapi `depends on !CC_IS_CLANG`; dengan toolchain clang tidak bisa diaktifkan. Mengganti toolchain ke gcc adalah keputusan terpisah, bukan Fase 1 ini |
| STRICT_KERNEL_RWX | y | y (tidak diubah) | Sudah aktif |
| STRICT_MODULE_RWX | tidak ada di .config | tidak diubah | `depends on MODULES`; modul off di defconfig ini, jadi tidak berlaku |
| SLAB_FREELIST_RANDOM | is not set | y | Diaktifkan Fase 1; depends `SLAB \|\| SLUB`, `SLUB=y` terpenuhi |
| SLAB_FREELIST_HARDENED | is not set | y | Diaktifkan Fase 1; depends `SLUB`, terpenuhi |
| DEBUG_LIST | y | y (tidak diubah) | Sudah aktif |
| BUG_ON_DATA_CORRUPTION | y | y (tidak diubah) | Sudah aktif; BUG berarti panic, itu perilaku baseline, bukan perubahan Fase 1 |
| CFI_CLANG | tidak ada di .config | TIDAK diaktifkan, menunggu keputusan | Berisiko tinggi (§7.2); `depends on LTO_CLANG` yang saat ini off, jadi mengaktifkannya juga butuh perubahan LTO. Jangan aktifkan tanpa persetujuan + uji boot manusia |
| SHADOW_CALL_STACK | is not set | TIDAK diaktifkan, menunggu keputusan | Berisiko tinggi (§7.2); secara Kconfig tersedia (`CC_HAVE_SHADOW_CALL_STACK=y`), jadi ini murni keputusan tertunda, bukan "tidak tersedia" |
| REFCOUNT_FULL | tidak ada di .config | tidak diubah | `ARCH_HAS_REFCOUNT_FULL=y` sudah dipilih arch, opsi user-nya disembunyikan (`if !ARCH_HAS_REFCOUNT_FULL`); implementasi penuh sudah dipakai, bukan opsi yang perlu diaktifkan |
| PANIC_ON_OOPS | y | y (tidak diubah) | Sudah aktif di baseline; AGENTS_4 mewajibkan tanya sebelum mengubahnya, dan di sini tidak ada yang diubah |

## Hasil build Fase 1

Build: `tools/rust-migration/build.sh
vendor/whyred-perf_defconfig out-fase1`, selesai
2026-11-03 05:38 WIB. Build sempat terinterupsi dua kali
sebelum selesai, dan keduanya dicatat, bukan disembunyikan:
1. Runtime restart membunuh build pertama setelah ~3 menit
   (baru 474 file `.o`, belum ada Image/vmlinux).
2. Build lanjutan gagal di awal karena `bc`/`bison`/`flex`
   hilang setelah pergantian VM; paket itu di-install ulang
   (`bc` 1.07.1, `bison` 3.8.2, `flex` 2.6.4, plus
   `libelf-dev` dan `dwarves`), lalu build dijalankan ulang
   ke `out-fase1` yang sama secara incremental.

Hasil akhir: exit sukses (LD vmlinux, SYSMAP, Image.gz-dtb
terbentuk), errors 0, warnings 7 baris log.

- `out-fase1/arch/arm64/boot/Image.gz-dtb`: 13.037.193 byte,
  sha256 `5beacb2068da0bbbd69b7557c06951fc3cdb9a43d46e724503b97443d76b4183`
  (baseline Fase 0: 13.025.530 byte, sha256 `62c0ec91...`;
  selisih ukuran +11.663 byte, wajar untuk perubahan config
  hardening, bukan bukti regresi dengan sendirinya).
- `out-fase1/vmlinux`: 39.652.008 byte
- `out-fase1/System.map`: 3.688.549 byte
- `.config` akhir terverifikasi: `HARDENED_USERCOPY=y`,
  `SLAB_FREELIST_RANDOM=y`, `SLAB_FREELIST_HARDENED=y`,
  `HARDENED_USERCOPY_FALLBACK=y` (default), `KSU=y`,
  `NOMOUNT=y`, dan tetap TIDAK ada simbol `KSU_SUSFS`
  (sama seperti baseline; SUSFS tetap off, lihat
  `baseline.md`).
- Perbandingan warning ternormalisasi vs
  `warnings-baseline.txt`: TIDAK ADA warning baru, dan
  TIDAK ADA warning yang hilang (5 baris sama persis).
  Jadi tidak ada warning baru tak terjelaskan dari
  perubahan config ini.

Catatan defconfig vendor (bukan akibat Fase 1):
defconfig mengandung simbol ganda sejak awal, misalnya
`SECURITY_SELINUX` dua kali dan `F2FS_FS_LZO` sekali
`is not set` lalu sekali `=y`. Saat defconfig dimuat,
Kconfig memberi warning `override: reassigning to symbol`
dan yang terakhir menang. Ini cacat bawaan defconfig
vendor, bukan dari 3 baris Fase 1, dan tidak fatal.
Membersihkannya adalah pekerjaan terpisah, bukan Fase 1.

## Uji manual yang perlu manusia lakukan

- Boot sampai layar kunci, 3x reboot stabil (daftar §12).
- Perhatikan panic baru yang bisa dipicu `HARDENED_USERCOPY`
  pada driver yang salah, dan perubahan performa heap
  dari freelist hardening (dampak diharapkan kecil).
- Jangan uji CFI/SCS: keduanya tidak diaktifkan di sini.

## Uji manual yang perlu manusia lakukan

- Boot sampai layar kunci, 3x reboot stabil (daftar §12).
- Perhatikan panic baru yang bisa dipicu `HARDENED_USERCOPY`
  pada driver yang salah, dan perubahan performa heap
  dari freelist hardening (dampak diharapkan kecil).
- Jangan uji CFI/SCS: keduanya tidak diaktifkan di sini.
