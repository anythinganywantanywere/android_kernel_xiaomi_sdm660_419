# Uji manual perangkat (§12 AGENTS_4) — 2026-11-03

Perangkat: Xiaomi Redmi Note 5 Pro (whyred), LineageOS Android 13.
Image yang diuji: `maya-agents4-test-whyred-twrp.zip`
(Image.gz-dtb sha256 `c363f88e...`, build 16:14 WIB 2026-11-03:
hardening Fase 1 + patch BUG-001, KSU=y, NOMOUNT=y,
SUSFS off, tanpa Rust di kernel).
Penguji: manusia (Ari). Agent tidak mem-flash perangkat;
semua perintah di bawah dijalankan manusia di Termux.

## Hasil yang dilaporkan manusia

### Boot dan identitas kernel — LULUS

- Boot sampai menu/layar kunci, dilaporkan aman.
- `uname -a`:
  `Linux localhost 4.19.325-st20-Maya-Kernel_Sienna-v2.5 #2 SMP PREEMPT Tue Nov 3 16:14:34 WIB 2026 aarch64`
- `/proc/version`:
  `Linux version 4.19.325-st20-Maya-Kernel_Sienna-v2.5 (root@htch-runtime) (Android (...) clang version 18.0.4, LLD 18.0.4) #2 SMP PREEMPT Tue Nov 3 16:14:34 WIB 2026`
- Waktu build 16:14:34 cocok dengan build patch BUG-001,
  jadi kernel yang jalan adalah image yang diuji,
  bukan kernel lama.

### Log kernel — LULUS DENGAN CATATAN

- `dmesg | grep -iE "BUG|WARNING|Oops|Unable to handle|Call Trace"`:
  **tidak ada** `BUG`, `Oops`, `Unable to handle`,
  atau panic.
- Dua `WARNING` dengan `Call trace` tercatat, keduanya
  tidak fatal (perangkat sampai menu):
  1. `place_entity` pada detik ke-0.006 (PID 1)
  2. `irq_startup` pada detik ke-50.4 (PID 5479)
- **Catatan yang belum terjawab:** apakah kedua WARNING
  itu sudah ada di kernel baseline atau baru muncul
  di image ini belum bisa dipastikan, karena tidak ada
  `dmesg` baseline untuk dibandingkan. Keduanya dicatat
  di sini sebagai tindak lanjut, bukan sebagai bug baru
  yang terbukti dan bukan sebagai alasan menggagalkan
  tes boot.
- Baris lain yang jinak dan bukan bug: warning EXT4
  soal mount count (perlu `e2fsck`, soal filesystem),
  `debugfs_ion does not exist` (debugfs memang tidak
  diaktifkan di kernel ini), dan warning kapabilitas
  32-bit `wpa_supplicant` (legacy biasa).

### Keamanan dan fitur — LULUS (yang teruji)

- `getenforce` = `Enforcing`;
  `/sys/fs/selinux/enforce` = `1`.
- `su` jalan (KernelSU root berfungsi); log NoMount
  menunjukkan hijack berhasil.
- Tool Rust `kernel-info` dijalankan di perangkat:
  - Tanpa root (Termux biasa): `release`, `version`,
    dan `machine` cocok persis dengan `uname`;
    `proc_version` kosong dan `selinux` = `unknown`
    karena aplikasi tanpa root tidak diizinkan membaca
    `/proc/version` dan `/sys/fs/selinux/enforce`
    (tool mengembalikan kosong, bukan mengarang nilai).
  - Dengan root (`su -c .../kernel-info`):
    `proc_version` terisi baris lengkap yang cocok
    dengan `/proc/version`, dan `selinux` = `enforcing`.
  - Kesimpulan: identitas kernel dan status SELinux
    terverifikasi lewat dua jalur independen
    (`uname`/`getenforce` dan `kernel-info`).

### Yang TIDAK dilaporkan secara terpisah

- Reboot 3x berturut-turut dan cek perangkat keras
  satu per satu (Wi-Fi, kamera, audio, fingerprint,
  dan seterusnya) **tidak** dilaporkan manusia secara
  terpisah dan rinci. Manusia menyatakan perangkat
  "aman" secara keseluruhan pada 16:46 WIB 2026-11-03,
  dan pernyataan itu yang dicatat di sini — bukan
  klaim bahwa setiap butir §12 sudah dicek satu per
  satu. Jika ada regresi perangkat keras yang muncul
  belakangan, butir-butir itu harus diuji ulang dan
  dilaporkan.

## Kesimpulan uji ini

Image AGENTS_4 (hardening + patch BUG-001) **lulus
uji boot dan identitas kernel** menurut bukti di atas:
boot aman, kernel yang benar, tidak ada BUG/Oops/
panic, SELinux enforcing, KernelSU dan NoMount
berfungsi, dan tool userspace terverifikasi di
perangkat. Dua WARNING `dmesg` dan butir §12 yang
tidak dilaporkan rinci tetap tercatat sebagai
tindak lanjut di atas, bukan disembunyikan.
