# Log keputusan (AGENTS_4 §5)

Entri baru di atas.

## D-008: Perbaikan BUG-001 hanya di file legacy yang terbangun
- Tanggal: 2026-11-03
- Status: disetujui manusia (implisit lewat perintah "selesaikan tugas AGENTS_4"; perbaikan minimal sesuai §10.1)
- Konteks: pola strcpy yang sama ada di camera_v2 dan camera_v2-legacy
- Opsi: (A) perbaiki keduanya, (B) perbaiki hanya legacy yang terbangun
- Keputusan: B. File legacy terbukti terbangun untuk whyred-perf dan perbaikannya terverifikasi lewat build; file non-legacy tidak terbangun di config ini, jadi mengubahnya tidak bisa diverifikasi build di sini
- Dampak/risiko: pola di camera_v2 tetap ada dan dicatat di BUG-001 sebagai tindak lanjut bila driver itu diaktifkan

## D-007: Pin toolchain whyred-rust-tools dihapus
- Tanggal: 2026-11-03
- Status: disetujui manusia (implisit; perbaikan atas kegagalan build)
- Konteks: rust-toolchain.toml dengan channel 1.99.1 membuat rustup mencari toolchain bernama 1.99.1 yang tidak memiliki komponen rustfmt/clippy di VM ini; toolchain terpasang bernama stable (versinya 1.99.1)
- Opsi: (A) unduh toolchain 1.99.1 + komponennya, (B) hapus pin, catat versi terverifikasi di README
- Keputusan: B. Versi terverifikasi (rustc/cargo 1.99.1) dicatat di README, bukan dipaksakan lewat toolchain file
- Dampak/risiko: reproduksibilitas versi menjadi catatan, bukan enforcement; dapat diterima untuk tool diagnostik kecil ini

## D-006: Rekomendasi Fase 5 — tunda kernel 6.x untuk Android
- Tanggal: 2026-11-03
- Status: diusulkan (menunggu keputusan manusia untuk riset lanjutan)
- Konteks: feasibility-6x.md
- Opsi: (A) lanjut ke Android 6.x, (B) tunda untuk Android, lanjut riset Linux non-Android, (C) batalkan port driver vendor ke Rust
- Keputusan: B + C. Alasan: tidak ada device tree whyred mainline, HAL/blob Android mengasumsikan kernel 4.19, dan tidak ada abstraksi Rust untuk WLAN/kamera/audio/GPU
- Dampak/risiko: tidak ada pekerjaan kernel 6.x yang dimulai dari dokumen ini; riset lanjutan butuh branch/repo terpisah dan persetujuan manusia

## D-005: Fase 2 — tidak ada batch merge
- Tanggal: 2026-11-03
- Status: disetujui manusia (implisit lewat perintah "selesaikan tugas AGENTS_4")
- Konteks: kernelversion lokal 4.19.325 = tag stable terbaru
- Opsi: (A) merge batch, (B) tidak ada batch, dokumentasikan posisi dan batasannya
- Keputusan: B. Remote stable tidak ditambahkan (jawaban didapat lewat ls-remote saja) dan tidak ada branch merge yang dibuat
- Dampak/risiko: klaim ini bukan paritas per file dengan upstream; perbaikan masa depan harus lewat cherry-pick bulletin Android

## D-004: Lokasi Rust userspace — repo terpisah (pilihan baku)
- Tanggal: 2026-11-03
- Status: diusulkan (pilihan baku §10.2 dipakai agar Fase 4 selesai; manusia boleh memindahkan ke userspace-rs/)
- Konteks: §10.2 mewajibkan bertanya soal lokasi, tetapi perintah "selesaikan tugas AGENTS_4" tidak menjawabnya
- Opsi: (A) repo terpisah whyred-rust-tools, (B) direktori userspace-rs/ di tree kernel
- Keputusan: A sementara. Workspace ada di ~/workspace/whyred-rust-tools/, repo git lokal sendiri, tanpa remote dan tanpa push; tree kernel tetap bersih
- Dampak/risiko: memindahkan ke userspace-rs/ nanti hanya soal memindahkan direktori, bukan menulis ulang kode

## D-003: SUSFS tetap off di track rust-migration
- Tanggal: 2026-11-03
- Status: ditolak untuk Fase 1-5 ini (belum diputuskan manusia; dibiarkan off, bukan diperbaiki diam-diam)
- Konteks: CONFIG_KSU_SUSFS=y di defconfig dibuang diam-diam dari .config karena Kconfig SUSFS tidak ada di tree sienna + KSU baru; perbaikan butuh perubahan di sekitar KernelSU
- Opsi: (A) biarkan off dengan catatan, (B) salin perbaikan Kconfig+shim dari rust-backport
- Keputusan: A untuk sekarang. Perbaikan SUSFS adalah perubahan di sekitar KernelSU dan di luar cakupan Fase 1-5 yang diperintahkan; menyalinnya diam-diam akan melanggar disiplin cakupan yang sudah dicatat di Fase 0/1
- Dampak/risiko: image track ini adalah KSU + NoMount TANPA SUSFS; jangan diklaim sebagai KSU+SUSFS

## D-002: CFI dan Shadow Call Stack tidak diaktifkan
- Tanggal: 2026-11-03
- Status: ditolak untuk Fase 1-5 ini (menunggu keputusan manusia yang eksplisit)
- Konteks: §3.2 dan §7.2 AGENTS_4 mewajibkan bertanya dulu; perintah "lanjutkan" dan "selesaikan tugas AGENTS_4" tidak menjawab pertanyaan itu secara spesifik
- Opsi: (A) aktifkan CFI, (B) aktifkan SCS, (C) aktifkan keduanya, (D) tidak aktifkan, dokumentasikan sebagai tertunda
- Keputusan: D. SHADOW_CALL_STACK tersedia secara Kconfig, CFI butuh LTO_CLANG yang off; keduanya berisiko bootloop pada driver vendor
- Dampak/risiko: hardening Fase 1 hanya mencakup opsi aman; CFI/SCS tetap menjadi satu-satunya keputusan hardening yang terbuka

## D-001: Track AGENTS_4 berjalan paralel, rust-backport tidak disentuh
- Tanggal: 2026-11-03
- Status: disetujui manusia (interpretasi atas "lakukan" 2026-11-03 04:46; dapat dikoreksi)
- Konteks: AGENTS_4 melarang isi AGENTS-2 (rust/, Rust Kconfig) di tree 4.19, sehingga AGENTS_4 tidak bisa dilanjutkan di atas rust-backport
- Opsi: (A) ganti rust-backport, (B) paralel dari sienna, (C) referensi saja
- Keputusan: B. Branch rust-migration/fase0-baseline dari sienna, lokal saja, tidak di-push, tidak di-flash oleh agent, rust-backport tidak disentuh
- Dampak/risiko: dua track hidup berdampingan; keputusan replace vs parallel vs reference-only secara formal masih terbuka, tetapi pekerjaan sudah berjalan di jalur paralel
