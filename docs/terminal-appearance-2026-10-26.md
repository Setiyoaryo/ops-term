# Pilihan font dan warna teks terminal

Buka Settings melalui **Ctrl+,**, lalu bagian Appearance.

- **UI theme** tetap mengatur antarmuka yang sudah ada.
- **Terminal font** memilih keluarga/bobot monospace yang tersedia: System default, JetBrains Mono Regular/Medium, Ubuntu Mono, DejaVu Sans Mono, Liberation Mono, atau Built-in monospace. Pilihan eksternal hanya ditampilkan bila file font tersedia dan dapat dibaca. Tidak ada unduhan font otomatis.
- **Terminal font size** tetap mengatur ukuran 10–30.
- **Text palette** menyediakan Original (warna sebelumnya), Soft (lebih redup dan rendah saturasi), Warm (warna tanah), dan Cool (biru abu-abu).
- **Text brightness** mengatur intensitas RGB foreground 55–100%, termasuk warna aplikasi truecolor dan highlight. Background dan warna tema UI tidak diredupkan oleh slider ini.
- Preview memakai font, ukuran dan resolver warna yang sama dengan terminal. Pilihan tersimpan otomatis dan berlaku pada output yang sudah ada.

Untuk mencoba gaya lebih ringan: **JetBrains Mono · Regular, Soft, brightness 85–90%**. Pilihan lama tetap dipertahankan saat migrasi: System default, Original, 100%.

## Implementasi

`egui/src/appearance.rs` menyimpan konfigurasi bertipe dengan default per field, katalog path font, palet dan kontrol preview. Pengaturan disimpan pada `ui.egui.terminal_appearance` di SQLite, bersama pengaturan UI yang sudah ada. Font/palet yang tidak dikenal mendapat default tanpa menghilangkan pengaturan lain. Font tersimpan yang tidak tersedia kembali memakai System default; brightness dibatasi ketika dimuat.

Hanya file font terminal yang dipilih dan font sans UI yang dimuat. Katalog menyimpan path, bukan seluruh bytes font. Pergantian font mengganti definisi dengan key yang sama pada pass egui berikutnya; pengukuran sel, gambar dan posisi pointer menggunakan font aktif yang sama. Sisa scroll pecahan direset saat pilihan font/ukuran berubah. Palet dan brightness tidak memuat ulang font. Batas atlas 2048 dari perbaikan sebelumnya tetap dipertahankan.

ANSI 16 warna (named dan indexed 0–15) mengikuti palet, termasuk background ANSI eksplisit. Indeks 16–255 dan truecolor mempertahankan warna aslinya sebelum brightness foreground diterapkan sekali, setelah highlight. Background default tetap mengikuti tema UI. Tidak ada penggantian framework atau pengurangan scrollback.

Settings kini dapat digulir dan dibatasi tinggi viewport agar pilihan tambahan serta Security tetap terjangkau pada jendela kecil.

## Verifikasi

Dua regresi penyimpanan/default terbukti gagal pada kode lama, lalu lulus sesudah integrasi. Suite egui release berisi 81 tes lulus, mencakup:

- round-trip pengaturan font, palet, brightness dan kompatibilitas pengaturan lama;
- partial object, enum yang tidak dikenal, brightness di luar batas dan font tidak tersedia;
- warna Original/Soft, kesamaan named/indexed ANSI dan preservasi extended color/truecolor;
- hasil render nyata yang memakai palet terpilih serta meredupkan highlight tepat sekali;
- regresi scroll, seleksi, fokus keyboard, warna ANSI/karakter lebar, dan idle repaint yang sudah ada.

Tes memakai direktori data sementara dan tidak menjalankan tes core vault yang memakai sesi keyring pengguna.

Uji GUI dengan Xvfb/xdotool memilih JetBrains Mono Regular, Soft dan brightness 85% melalui klik asli; SQLite menyimpan ketiganya, warna ANSI pada terminal berubah sesuai resolver, dan pilihan tetap tampil setelah restart proses. Screenshot pada ukuran minimum 760×500 membuktikan scroll Settings mencapai kontrol Security paling bawah. Screenshot sesi berada di `/tmp/opsterm-appearance-verification/`. Review independen tidak menemukan blocker; fallback file font yang tidak bisa dibaca telah dirapikan.

Build release, Clippy release all-targets `-D warnings`, format check dan smoke shell PTY asli juga lulus. Tes penampilan dapat diulang dengan `scripts/smoke-egui-appearance.py` (lihat prasyarat dan command pada docstring). Binary dipasang secara atomik ke `~/.local/bin/opsterm-egui`; rollback sebelum perubahan ini berada di `~/.local/bin/opsterm-egui.rollback-before-appearance-20261026`. Aplikasi yang sedang berjalan tidak dihentikan.

Cache build/test sudah dibersihkan memakai `npm run clean` di bawah lock build; `target/`, `src-tauri/target/` dan cache Python tidak tertinggal. Screenshot di `/tmp/opsterm-appearance-verification/` dipertahankan untuk review.
