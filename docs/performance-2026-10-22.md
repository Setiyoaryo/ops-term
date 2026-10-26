# egui: respons perangkat, tampilan, dan penggunaan resource

Pembaruan kedua, 22 Oktober 2026. Arah pilihan pengguna: **modern gelap minimal**.
Laporan verifikasi sebelumnya tetap tersedia sebagai catatan versi terdahulu.

## Perubahan perilaku dan tampilan

- Satu klik pada nama/host perangkat langsung membuat atau memfokuskan tab.
  DNS, keyring, transport, dan autentikasi berjalan di worker; tidak perlu
  menunggu worker tersedia untuk melihat tab. Status Connecting dan kegagalan
  tampil di tab tersebut. Waktu autentikasi/server tetap bergantung pada jaringan.
- Klik berulang memakai tab yang sedang terbuka. Menu klik kanan menyediakan
  koneksi tambahan. Penyelesaian koneksi tidak mencuri fokus tab lain dan ID
  tab tetap sama. Hasil untuk tab yang sudah ditutup dibersihkan.
- Toolbar Quick Connect digabung dengan menu utama; tab memakai garis aksen,
  sidebar berupa baris rata kiri, terminal datar tanpa kartu membulat, dan
  composer dimulai dalam keadaan tertutup. Command/Ctrl+Shift+P membukanya.
- Daftar perangkat disimpan di cache dan hanya baris terlihat yang digambar.
  Edit/import memperbarui cache; Refresh mengambil perubahan klien lain.
- Repaint mengikuti input/output/hasil worker, dengan timer idle satu detik.
  Output dijadwalkan dalam 16 ms, termasuk sisa burst setelah reader berhenti.
  Tool/runbook yang aktif memakai interval 50 ms. Tidak ada loop idle 60 FPS.
- Teks terminal digabung menurut warna, tetap mempertahankan kolom karakter
  lebar. Scrollback dibatasi 2.000 baris; antrean output 16 × 8 KiB per sesi.
  Status menampilkan RSS proses Linux, bukan perkiraan penggunaan seluruh sistem.

## Perbandingan terukur

Release sebelumnya disimpan sebelum perubahan di
`/tmp/opsterm-pass2/opsterm-before`. Kedua binary diuji **berurutan** memakai
`scripts/measure-egui.py`, Xvfb 1280×850, jendela aplikasi 1180×760,
`LIBGL_ALWAYS_SOFTWARE=1`, database sementara berisi 500 perangkat SSH sintetis,
dan satu shell lokal. Tidak ada koneksi ke perangkat jaringan. Setelah lima
detik pemanasan, CPU diambil selama lima detik. Lalu shell mencetak 12.000 baris,
ditunggu selesai dan dua detik untuk drain, kemudian sampel idle lima detik lagi.

| Metrik proses aplikasi | Sebelum | Sesudah |
| --- | ---: | ---: |
| CPU idle awal (100% = satu inti) | 308,32% | 9,40% |
| RSS idle awal | 122,52 MiB | 119,63 MiB |
| PSS idle awal | 77,86 MiB | 75,12 MiB |
| Private dirty idle awal | 45,07 MiB | 41,38 MiB |
| CPU idle setelah output | 297,16% | 7,20% |
| RSS setelah output | 145,55 MiB | 122,93 MiB |
| PSS setelah output | 100,86 MiB | 78,39 MiB |
| Private dirty setelah output | 68,04 MiB | 44,49 MiB |

Pada fixture ini RSS setelah output turun **15,5%**, PSS **22,3%**, dan CPU
idle setelah output **97,6%**. RAM idle awal turun lebih kecil, sekitar 2,4%.
Angka CPU tinggi pada versi sebelumnya berasal dari software rendering yang
memakai beberapa inti; bukan pengukuran GPU desktop pengguna. Sampel ini satu
pengulangan per binary dan bukan jaminan untuk setiap driver/ukuran terminal.
RSS/PSS berasal dari `/proc/PID/smaps_rollup`; CPU dari `/proc/PID/stat`.
Keduanya mengecualikan proses shell anak, Xvfb, dan aplikasi lain. Pengurangan
scrollback memang mengurangi jumlah riwayat yang dapat digulir.

Ulangi dengan binary yang ingin dibandingkan:

```bash
python3 scripts/build_guard.py -- timeout 60s xvfb-run -a -s '-screen 0 1280x850x24' python3 scripts/measure-egui.py target/release/opsterm-egui 500
```

## Verifikasi final

| Pemeriksaan | Hasil |
| --- | --- |
| Core + egui, offline/locked, satu thread tes | 47 + 52 tes lulus; puncak RSS group 834,6 MiB |
| Clippy core + egui, all-targets, `-D warnings` | Exit 0; puncak 485,7 MiB |
| `npm run build:egui -- --offline` | Exit 0, 18,28 detik; puncak 543,1 MiB |
| Smoke GUI release asli | Lulus; puncak group 214,7 MiB |
| Build guard | 5 tes lulus |
| Format Rust, sintaks Bash/Python | Exit 0 |

Regresi membuktikan tab langsung tersedia dalam batas 50 ms pada pengujian
lokal meskipun empat worker masih ditahan; ini mengukur aksi aplikasi, bukan
latensi jaringan. Tes juga meliputi klik nama/host, fokus ulang tanpa duplikasi,
completion tanpa pencurian fokus, cleanup target tertutup, idle repaint,
cache setelah edit, batas riwayat, batching shape, ANSI/Unicode, serta burst
output yang sebelumnya menunggu timer idle. Regresi burst gagal sebelum fix.

Smoke `scripts/smoke-egui.sh` memakai database sementara dan shell asli: Ctrl+C
menginterupsi sleep, Enter composer mengeksekusi, klik satu baris perangkat lokal
membuka shell kedua, dan berpindah tab benar-benar mengganti target perintah.
Screenshot release ukuran normal/minimum disimpan di `test-results/egui/`.
Preview fixture: [normal](../test-results/egui/modern-desktop.png),
[minimum](../test-results/egui/modern-minimum.png), dan
[tab sesaat setelah klik perangkat](../test-results/egui/modern-device-click.png).
Tidak ada OOM pada build/verifikasi ini. Guard satu build aktif tetap digunakan.
Ada pemberitahuan future incompatibility dari dependency russh 0.54.5;
Clippy kode proyek lulus tanpa warning.

Review akhir memeriksa kepemilikan provider saat tab ditutup, stabilitas ID
dan target broadcast, penyegaran cache, backpressure dan wake output, serta
kerapian tampilan normal/minimum. Autentikasi dan pemeriksaan host key tidak
diubah. Tidak ada dependency baru atau perubahan data perangkat pengguna.

Release terbaru dijalankan pada desktop pengguna melalui unit
`opsterm-preview-v2.service`; status terverifikasi active/running.
