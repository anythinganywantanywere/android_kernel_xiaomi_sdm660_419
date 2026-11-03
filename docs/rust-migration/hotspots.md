# Titik rawan memori (Fase 3 AGENTS_4)

Tanggal audit: 2026-11-03.
Metodologi: §9.1 AGENTS_4, dengan batasan yang dicatat
jujur di bawah. Urutan berdasarkan skor, bukan ukuran kode.

## Alat yang dipakai dan yang tidak tersedia

- Dipakai: `grep` terarah pada pola berisiko
  (`copy_from_user`/`copy_to_user`, `strcpy`/`sprintf`,
  `memcpy`), `wc -l` untuk ukuran, dan bukti objek
  terbangun di `out-fase1` (apakah direktori itu
  benar-benar dikompilasi untuk whyred-perf).
- `scripts/checkpatch.pl` tersedia di tree, tetapi tidak
  dipakai sebagai bukti bug: checkpatch menilai gaya,
  bukan keamanan, dan menjalankannya ke seluruh driver
  vendor hanya menghasilkan noise gaya.
- TIDAK tersedia dan TIDAK dijalankan: `sparse`
  (`command -v sparse` kosong) dan `spatch`/coccinelle
  (kosong). Jadi tidak ada klaim hasil sparse atau
  coccicheck di dokumen ini.
- Build `W=1` per subset tidak dijalankan sebagai
  langkah audit terpisah: build penuh Fase 1 sudah
  memberi daftar warning, dan tidak ada warning baru
  di sana. Menjalankan `W=1` ke seluruh qcacld/techpack
  akan menghasilkan ratusan warning gaya yang tidak
  membedakan bug dari noise tanpa penelaahan manual
  per warning, yang tidak dilakukan di sini.

## Cara membaca skor

```
skor = 2 * Paparan + Manipulasi + Riwayat
Paparan:     3 = bisa dijangkau remote (jaringan, radio, USB host, firmware)
             2 = node/ioctl terjangkau aplikasi biasa
             1 = hanya proses berhak istimewa / rantai boot
             0 = internal saja
Manipulasi:  0..3  seberapa banyak aritmetika buffer/pointer manual
             (di sini diproksi dari jumlah baris copy_from/to_user
             dan pola string tak terbatas yang ditemukan grep,
             bukan dari penelaahan tiap fungsi)
Riwayat:     0..2  CVE/patch upstream/bulletin terkait
```

**Catatan penting tentang Riwayat:** semua baris di bawah
ini memakai Riwayat = 0, karena audit ini **tidak
memverifikasi satu pun CVE spesifik** untuk driver-driver
tersebut. Itu bukan klaim "tidak ada CVE"; itu berarti
skor di sini adalah batas bawah dan urutannya didorong
oleh Paparan + Manipulasi. Jangan mengutip skor ini
sebagai "sudah dicek CVE-nya".

**Catatan tentang hitungan:** LOC = `wc -l` seluruh
`.c`/`.h` di direktori (termasuk header dan kode yang
mungkin tidak terbangun untuk whyred). `built_objs` =
jumlah `.o` di `out-fase1` untuk direktori itu, bukti
bahwa direktori tersebut memang dikompilasi di build
Fase 1. `copy_lines` = jumlah baris yang mengandung
`copy_from_user` atau `copy_to_user` menurut grep
(per file, dijumlahkan) — indikator permukaan, bukan
jumlah bug.

## Daftar berskor

Format: `path | fungsi/driver | paparan | manipulasi | riwayat | skor | catatan | ukuran (LOC)`

- `drivers/staging/qcacld-3.0` | WLAN | 3 | 3 | 0 | **9** | Parser frame/perintah dari jaringan dan firmware; permukaan remote terbesar. Prioritas penelaahan manual berikutnya. | 670863 (287 objek terbangun, 59 baris copy)
- `net/bluetooth` | Bluetooth | 3 | 2 | 0 | **8** | Radio, jangkauan remote; parsing paket dan ioctl. | 56808 (15 objek, 66 baris copy)
- `techpack/audio` | Audio | 2 | 3 | 0 | **7** | Ioctl dan parser kontrol audio; jumlah baris copy tertinggi kedua. | 457173 (136 objek, 493 baris copy)
- `drivers/media/platform/msm` | Kamera/media | 2 | 3 | 0 | **7** | Ioctl kamera dan parser sensor/EEPROM; jumlah baris copy tertinggi. Satu pola tak terbatas yang terverifikasi menjadi BUG-001, tetapi pola itu sendiri hanya terjangkau lewat device tree (lihat BUG-001), bukan lewat ioctl aplikasi. | 373606 (65 objek, 589 baris copy)
- `drivers/char` | char/diag/adsprpc dan sejenisnya | 2 | 3 | 0 | **7** | Node char yang terjangkau aplikasi; tumpang tindih sebagian dengan soc/qcom (diag/adsprpc) — jangan dijumlahkan ganda. | 136205 (21 objek, 262 baris copy)
- `drivers/gpu/msm` | GPU/kgsl | 2 | 2 | 0 | **6** | Ioctl GPU dan manajemen memori (ION/dma-buf). | 72298 (34 objek, 44 baris copy)
- `drivers/soc/qcom` | SMEM, glink/smd, IPC router | 2 | 2 | 0 | **6** | Antarmuka antar-subsistem Qualcomm; sebagian node terjangkau aplikasi. | 120826 (59 objek, 54 baris copy)
- `drivers/android` | Binder | 2 | 2 | 0 | **6** | Dipakai semua aplikasi, tetapi dimediasi SELinux dan sudah menjadi fokus upstream; penelaahan di sini bernilai lebih rendah daripada driver vendor. | 11573 (4 objek, 26 baris copy)
- `drivers/usb/gadget` | USB gadget | 2 | 2 | 0 | **6** | Dikendalikan host USB, bukan aplikasi biasa; permukaan tergantung mode gadget yang aktif. | 164008 (28 objek, 65 baris copy)
- `fs/ext4` + `fs/f2fs` | Filesystem | 2 | 1 | 0 | **5** | Parser citra dari media/USB; mount memerlukan hak istimewa, tetapi citra tidak tepercaya. | 55190 + 46532 (33 + 20 objek, 13 + 15 baris copy)
- `drivers/staging/android` | ashmem/ion dan sejenisnya | 2 | 1 | 0 | **5** | Antarmuka memori bersama untuk aplikasi. | 7916 (14 objek, 18 baris copy)
- `drivers/input/touchscreen` | Touch | 1 | 1 | 0 | **3** | Input perangkat keras; parsing firmware touch ada di sini, tetapi tidak terjangkau aplikasi langsung. | 187015 (8 objek, 29 baris copy)
- `drivers/kernelsu` (dan `fs/susfs.c`, `fs/nomount`) | KernelSU | 1 | 2 | 0 | **4** | **CATAT SAJA, JANGAN UBAH** (§9.2 AGENTS_4 dan §3.3.7). Skor dicantumkan hanya untuk kelengkapan inventaris, bukan untuk antrean perbaikan. | tidak dihitung di sini

## Kandidat yang tidak ada di tree ini

- `sound/soc/msm` tidak ada sebagai direktori; audio
  untuk SoC ini ada di `techpack/audio` (sudah masuk
  daftar di atas). Jangan mencari `sound/soc/msm`.
- Driver fingerprint khusus whyred tidak dipisahkan
  sebagai direktori sendiri dalam daftar §9.2; ia
  tercakup dalam penelaahan `drivers/input` dan
  `drivers/misc` secara umum, dan tidak diberi baris
  sendiri karena tidak teridentifikasi sebagai
  direktori fingerprint yang berdiri sendiri.

## Pola yang sudah diperiksa dan bukan bug

- `sprintf(sensor_fusion_id, "%04x...")` di
  `msm_sensor_driver.c` (kamera): buffer tujuan adalah
  `static char sensor_fusion_id[512]`, format lebar
  tetap menghasilkan 44 karakter + NUL. Bukan overflow;
  menggantinya ke `snprintf` hanya perbaikan gaya,
  bukan perbaikan keamanan, dan tidak dibuatkan
  BUG-NNN.
- `sprintf(buf, "%pa\n", ...)` di `drivers/soc/qcom/rmtfs_mem.c`:
  fungsi show sysfs dengan buffer `PAGE_SIZE`; format
  dan argumen terbatas. Bukan bug yang terverifikasi.

## Temuan yang terverifikasi sebagai kandidat perbaikan

- **BUG-001** (`docs/rust-migration/bugs/BUG-001.md`):
  `strcpy` tanpa batas dari string device tree ke
  `eeprom_name[128]` di driver EEPROM kamera legacy
  (yang terbangun untuk whyred-perf). Prioritas rendah
  karena hanya terjangkau lewat device tree, bukan
  lewat aplikasi atau jaringan. Tidak ada exploit atau
  PoC yang ditulis, sesuai §9.4.
- Tidak ada bug lain yang terverifikasi di audit ini.
  Tidak adanya BUG-002 dst. berarti "tidak ada bug
  lain yang terbukti dari pola yang diperiksa", bukan
  "driver-driver ini bebas bug".

## Yang lebih cocok dipindah ke userspace

Tidak ada kandidat yang diusulkan untuk dipindah ke
userspace dari audit ini. Semua driver di atas adalah
driver kernel yang memang harus di kernel (WLAN,
audio, kamera, GPU, binder). Yang dipindah ke userspace
di Fase 4 adalah **alat baru** (diagnostik), bukan
driver yang ada — sesuai §10.2.
