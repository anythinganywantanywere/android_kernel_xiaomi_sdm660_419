# Kelayakan kernel 6.x dengan Rust untuk SDM660 / whyred (Fase 5)

Dokumen saja, sesuai §11 AGENTS_4. Tidak ada driver
vendor yang dipindahkan ke Rust di sini, dan tidak ada
perubahan pada tree 4.19 ini dari dokumen ini.
Tanggal pengecekan sumber: 2026-11-03.

## Ringkasan rekomendasi

**TUNDA** untuk Android harian di whyred.
**LANJUT sebagai riset** hanya untuk Linux non-Android
(postmarketOS dan sejenisnya), dengan target kernel
yang lebih baru dari 6.1.
**BATALKAN** untuk gagasan memindahkan driver vendor
yang ada (WLAN, kamera, audio, KGSL) ke Rust: tidak ada
abstraksi Rust untuk subsistem-subsistem itu, dan
memindahkannya bukan syarat untuk memakai kernel 6.x.

## 1. Status SDM660 dan whyred di kernel 6.1 ke atas

- SDM660 bukan SoC yang didukung penuh di kernel
  mainline torvalds. Yang ada adalah pekerjaan komunitas:
  fork `sdm660-mainline/linux` di GitHub (terakhir
  diperbarui 26 Oktober 2026 menurut daftar organisasi
  tersebut), dukungan clock GPU SDM660 di mainline,
  dan dukungan modem SDM630/636/660 di driver
  remoteproc `q6v5-mss` yang masuk untuk 6.6.
  Sumber: daftar organisasi sdm660-mainline di GitHub;
  log remoteproc 6.6 (commit `10b6fec2` dkk., Juni 2023).
- Perangkat whyred sendiri: postmarketOS mengenal
  `xiaomi/whyred` sebagai perangkat (pmbootstrap,
  `linux-xiaomi-whyred`), tetapi itu kernel downstream
  untuk whyred, bukan device tree whyred di mainline
  torvalds. Dari pengecekan ini, **tidak ditemukan**
  device tree whyred di mainline torvalds. Jangan
  mengklaim "whyred sudah mainline".
- Perangkat SDM660 lain yang lebih maju mainline-nya
  adalah lavender (Redmi Note 7, SDM660): patch fuel
  gauge PMI8998 pada November 2025 menyebut
  `sdm660-xiaomi-lavender` secara eksplisit. Lavender
  bukan whyred; kemajuan lavender tidak otomatis
  berlaku untuk whyred (panel, sensor, kamera, dan
  konfigurasi board berbeda).
- Kesimpulan bagian ini: basis yang realistis untuk
  riset 6.x adalah fork komunitas sdm660-mainline,
  bukan torvalds murni, dan whyred masih memerlukan
  pekerjaan device tree sendiri.

## 2. Driver yang masih bergantung blob / kode vendor

| Fungsi | Status di 6.x / mainline | Dampak untuk whyred |
|---|---|---|
| Display | Panel dan DSI vendor; display mainline untuk SDM660 masih pekerjaan komunitas, belum setara downstream | Tanpa panel yang jalan, tidak ada UI |
| Kamera | Driver kamera MSM vendor (`camera_v2-legacy` di tree ini) tidak ada padanannya di mainline; tidak ada driver kamera mainline untuk sensor whyred | Kamera hilang total di mainline |
| Audio | Audio vendor (techpack) tidak ada di mainline; audio mainline SDM660 masih terbatas | Audio panggilan/media hilang atau terbatas |
| Modem | Remoteproc MSS SDM630/636/660 masuk 6.6, tetapi modem yang jalan penuh (telepon, data, SMS) butuh firmware dan userspace yang sesuai, bukan hanya driver remoteproc | Telepon/data tidak dijamin oleh satu driver |
| GPU | **Koreksi terhadap §11 AGENTS_4:** whyred memakai Snapdragon 636, jadi GPU-nya Adreno **509**, bukan 508 (508 = SDM630, 512 = SDM660, menurut sumber Phoronix di bawah). Driver MSM/Freedreno mendukung A508/A509/A512 sejak Linux 5.12 | GPU adalah bagian yang paling maju di mainline, tetapi hanya untuk Linux non-Android (Mesa); blob Adreno Android tidak memakai MSM DRM |
| WLAN | qcacld-3.0 vendor tidak ada di mainline; WLAN mainline untuk SDM660 bergantung pada firmware dan driver yang berbeda | Wi-Fi tidak dijamin setara downstream |

## 3. Dampak ke userspace Android

- Vendor blob Android yang dipakai whyred sekarang
  (LineageOS Android 13 di perangkat ini) dibangun
  dengan asumsi antarmuka kernel 4.19: HAL kamera,
  audio, grafis (gralloc/KGSL), dan RIL/modem.
  Pindah ke kernel 6.x berarti blob-blob itu tidak
  lagi berbicara ke kernel yang sama — bukan sekadar
  "ganti kernel, ROM tetap jalan".
- Jalur Android yang benar untuk kernel baru adalah
  GKI atau vendor kernel yang didukung vendor HAL,
  yang tidak tersedia untuk whyred dari Qualcomm/
  Xiaomi. Jadi target Android 6.x untuk whyred adalah
  proyek porting userspace juga, bukan proyek kernel
  saja.
- Jalur yang tidak memerlukan blob Android adalah
  Linux non-Android (postmarketOS): display lewat
  DRM, audio lewat ALSA, grafis lewat Mesa/Freedreno.
  Di jalur inilah kernel 6.x masuk akal.

## 4. Status Rust-for-Linux pada versi target

- **6.1: terlalu dini untuk driver nyata.** Rust masuk
  6.1 hanya sebagai infrastruktur dan modul contoh
  "hello world"; tidak ada driver produksi di 6.1,
  dan tidak ada abstraksi driver yang cukup untuk
  menulis driver perangkat nyata. Memilih 6.1 sebagai
  target Rust adalah kesalahan: 6.1 adalah titik awal
  sejarah Rust di kernel, bukan titik awal yang layak
  untuk pengembangan driver.
- **6.8 ke atas: mulai ada driver Rust nyata**
  (misalnya PHY jaringan), dan abstraksi platform,
  PCI, I2C, USB, dan serdev sudah ada di kernel.
- **SPI: belum merged saat pengecekan ini.**
  Seri patch abstraksi SPI Rust v6 tertanggal
  27 Oktober 2026 masih berupa patch series untuk
  merge window 7.4, dengan pernyataan eksplisit
  bahwa subsistem SPI "masih kekurangan abstraksi"
  sementara PCI, platform, I2C, USB, dan serdev
  sudah ada. Jadi driver SPI Rust belum bisa ditulis
  terhadap kernel yang sudah rilis saat dokumen ini
  dibuat, tanpa membawa patch tersebut sendiri.
- **GPIO:** abstraksi GPIO Rust ada dalam pekerjaan
  komunitas (terlihat di riwayat kontribusi Rust
  for Linux), tetapi bukan abstraksi yang matang
  dan terdokumentasi setara platform/I2C pada
  versi target awal.
- **Yang tidak ada abstraksi Rust-nya sama sekali:**
  WLAN (mac80211/cfg80211), kamera (V4L2/media),
  audio (ALSA SoC), GPU DRM/KGSL, dan binder
  (kecuali pekerjaan binder Rust Android yang
  terpisah dan bukan abstraksi driver umum).
  Artinya: tidak ada satu pun driver prioritas
  whyred yang bisa ditulis ulang ke Rust dengan
  abstraksi yang tersedia.

## 5. Kandidat driver BARU yang cocok ditulis Rust

Hanya driver baru yang kecil, tidak bergantung pada
subsistem di atas, dan tidak menggantikan driver
vendor yang ada:

1. Driver platform sederhana untuk perangkat
   I2C/SPI kecil yang belum punya driver (misalnya
   sensor atau pengatur sederhana) — itupun hanya
   setelah abstraksi I2C/SPI tersedia di kernel
   target, dan hanya di tree riset, bukan di
   tree 4.19 ini.
2. Modul contoh/platform untuk pembelajaran di
   kernel riset (bukan untuk perangkat produksi).
3. Tidak ada kandidat dari daftar hotspot Fase 3
   yang cocok: WLAN, audio, kamera, dan GPU semuanya
   bergantung pada subsistem tanpa abstraksi Rust.

## 6. Estimasi usaha, risiko, rekomendasi

- **Riset Linux non-Android (postmarketOS) sampai
  boot + display + Wi-Fi dasar:** estimasi kasar
  berminggu-minggu sampai berbulan-bulan untuk
  satu pengembang yang sudah paham mainline
  Qualcomm, dengan ketidakpastian besar di panel,
  audio, dan modem. Ini estimasi, bukan janji;
  tidak ada pengukuran yang mendasarinya di
  dokumen ini.
- **Android harian setara 4.19 sekarang:** tidak
  dapat diestimasi secara wajar tanpa dukungan
  vendor; risikonya adalah proyek tanpa ujung
  (porting HAL dan blob) dan harus dianggap
  **tidak layak** sebagai kelanjutan dari
  pekerjaan AGENTS_4 ini.
- **Risiko utama:** brick/bootloop dari device tree
  yang salah, regresi perangkat keras yang tidak
  bisa diuji tanpa perangkat (panel, modem),
  dan beban pemeliharaan fork komunitas yang
  bergerak cepat.
- **Rekomendasi akhir:** TUNDA kernel 6.x untuk
  whyred sebagai target Android; LANJUTKAN hanya
  sebagai riset Linux non-Android di branch/repo
  terpisah, dengan target kernel di atas 6.1
  (praktisnya 6.12 atau yang lebih baru saat
  abstraksi I2C/platform sudah matang), dan
  BATALKAN gagasan port driver vendor ke Rust.

## Sumber (dicek 2026-11-03)

- Organisasi sdm660-mainline di GitHub (linux fork,
  diperbarui 26 Okt 2026): https://github.com/sdm660-mainline
- Remoteproc q6v5-mss SDM630/636/660 untuk 6.6:
  https://git.ti.com/cgit/ti-linux-kernel/ti-linux-kernel/atom/drivers/remoteproc/qcom_q6v5_mss.c?h=ti-linux-6.6.y
- Seri patch fuel gauge PMI8998 (lavender, Nov 2025):
  https://lwn.net/Articles/1048226/
- postmarketOS whyred (pmbootstrap, kernel downstream):
  https://ivonblog.com/en-us/posts/postmarketos-netboot/
- Adreno A508/A509/A512 di MSM DRM untuk Linux 5.12
  (509 = SDM636, koreksi untuk whyred):
  https://www.phoronix.com/news/MSM-DRM-For-Linux-5.12
- Rust di Linux 6.1 (infrastruktur + contoh saja):
  https://linux.slashdot.org/story/22/12/12/0313228/linux-61-released-with-initial-support-for-rust-based-kernel-development?sdsrc=nextbtmprev
- Abstraksi SPI Rust, patch series v6, 27 Okt 2026
  (belum merged, target 7.4):
  https://lists.openwall.net/linux-kernel/2026/10/27/421
- Abstraksi I2C Rust, patch series v7, 10 Nov 2025:
  https://lkml.indiana.edu/2511.1/01636.html

Catatan kualitas sumber: sumber Rust 6.1 di atas
adalah liputan sekunder (Slashdot/LWN); klaim
"6.1 hanya infrastruktur" konsisten dengan riwayat
merge Rust-for-Linux yang diketahui umum, tetapi
bukan kutipan commit primer. Klaim SPI/I2C di atas
adalah kutipan primer dari patch series itu sendiri.
