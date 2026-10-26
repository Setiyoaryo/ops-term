# egui hover, scroll, dan memori — 26 Oktober 2026

Tampilan tetap memakai egui, font dan tema yang sama, tanpa mengurangi scrollback 2.000 baris.

## Perubahan

- Tombol tanpa latar seperti X dialog kini memakai garis hover/pressed berwarna aksen yang lebih tebal. Ukuran hit area dan perilaku jendela egui tetap sama.
- Kecepatan roda mouse sidebar/terminal menjadi 2× pada nilai slider yang sama, termasuk pengaturan tersimpan. Input trackpad tetap memakai delta titik dari egui.
- Terminal menggambar pecahan piksel di antara perpindahan baris, termasuk baris riwayat parsial di atas viewport. Koordinat seleksi/hover mengikuti posisi gambar; Select all menyertakan baris parsial. Scroll dibatasi di kedua ujung tanpa akumulasi overscroll; input keyboard dan resize mereset sisa pecahan.
- Animasi tetap memakai smoothing egui dan jadwal repaint 16 ms saat scroll aktif. Ini target sekitar 60 FPS, bukan hasil pengukuran presentasi GPU; frame rate aktual bergantung pada driver dan beban desktop. Idle tetap event-driven.
- Atlas font dibatasi lebar 2.048 piksel atau batas driver yang lebih kecil. Resolusi glyph, ukuran font dan antialiasing tetap sama; atlas dapat bertambah tinggi. Font dipasang sebelum frame pertama agar font sementara tidak perlu dirasterisasi dulu.
- Render terminal mengambil tema sekali per frame, menghindari ribuan penguncian mutex warna per frame.
- Sumber awal juga memiliki referensi `trimmed` yang tidak terdefinisi dan inisialisasi tes `Paint` tanpa `hover`; keduanya dibetulkan agar build bisa dijalankan.

## Pengukuran

Rilis lama: `release/opsterm-egui-0.1.0-linux-x86_64`. Rilis baru dibangun memakai `npm run build:egui`.

Keduanya dijalankan berurutan dengan database sementara, nol perangkat tersimpan dan satu shell lokal. Pemanasan lima detik, sampel CPU idle lima detik. RSS/PSS/private dirty dibaca dari `/proc/PID/smaps_rollup`; CPU dari `/proc/PID/stat`. Tidak termasuk proses shell anak. Satu pengulangan per kondisi, sehingga ini bukan jaminan lintas perangkat.

| Kondisi | RSS lama → baru | PSS lama → baru | Private dirty lama → baru | CPU lama → baru |
| --- | --- | --- | --- | --- |
| Desktop/GPU asli, idle | 94,61 → 90,42 MiB | 50,10 → 45,96 MiB | 31,54 → 27,77 MiB | 0,20 → 0,20% |
| Xvfb/software GL, idle | 121,86 → 116,25 MiB | 75,19 → 69,57 MiB | 43,48 → 37,95 MiB | 5,40 → 5,20% |
| Xvfb/software GL, sesudah 12.000 baris output | 124,09 → 121,16 MiB | 77,42 → 74,42 MiB | 45,71 → 42,79 MiB | 6,00 → 6,00% |

Pada desktop asli, RSS turun sekitar 4,4%, private dirty sekitar 12,0%. **Target total RSS 15–30 MiB belum tercapai.** Proses pengguna yang diamati memetakan sekitar 40 MiB resident LLVM driver saja; angka status tetap RSS, tidak diganti menjadi private memory supaya terlihat kecil. Tidak ada pemotongan kualitas UI atau pengurangan keamanan vault untuk mengejar angka tersebut.

Pengukuran software bisa diulang dengan:

```bash
python3 scripts/build_guard.py -- timeout 60s xvfb-run -a -s '-screen 0 1280x850x24' python3 scripts/measure-egui.py target/release/opsterm-egui 0
```

## Verifikasi

- Tiga regresi awal terbukti gagal sebelum perbaikan: stroke hover identik, batas atlas mengikuti driver 16K, posisi teks mengabaikan pecahan scroll. Ketiganya lulus sesudahnya.
- 54 tes core + 71 tes egui lulus (filesystem pengujian terisolasi). Termasuk clamp kedua ujung riwayat dan reverse scroll tanpa dead zone, warna ANSI/Unicode, input/fokus/seleksi dan idle repaint.
- Clippy core/egui all-targets dengan `-D warnings` lulus; peringatan lama nested-if/borrow dan `map_or` dirapikan tanpa perubahan perilaku.
- Build release lulus. Dependency `russh 0.54.5` masih memberi pemberitahuan future incompatibility.
- Smoke GUI rilis lulus: shell PTY asli, Ctrl+C, Enter composer, klik perangkat lokal kedua dan target tab terisolasi, layout normal/minimum.
- Tes GUI tambahan menggunakan Xvfb/xdotool membandingkan screenshot X normal/hover, mengkliknya, lalu memastikan dialog tertutup. Screenshot diperiksa secara visual.
- Review independen memeriksa clipping/iterator baris ekstra, arah pecahan dan koordinat pointer, konsumsi wheel delta, serta kapasitas atlas. Temuan Select all diperbaiki. Kapasitas glyph Unicode ekstrem pada DPI sangat tinggi belum di-stress-test.

Hasil pengukuran mentah dan screenshot sesi ini berada di `/tmp/opsterm-verification/`. Cache build dibersihkan setelah binary dan laporan selesai digunakan.

## Catatan keyring saat pengujian

Tes vault bawaan memakai entri sesi OS keyring yang sama dengan aplikasi, meskipun file vault tes berada di direktori sementara. Tes dapat menghapus/mengganti kunci sesi auto-resume; saat aplikasi dibuka ulang, master password mungkin diminta lagi. File vault pengguna dan secret terenkripsinya tidak diubah. Ini ditemukan setelah tes core berjalan; aplikasi pengguna yang sudah terbuka tidak dihentikan.
