# Verifikasi perbaikan OpsTerm — 22 Oktober 2026

Prioritas sesuai pilihan pengguna: klien **egui**. Perubahan dikerjakan langsung
karena direktori ini tidak memiliki metadata Git. Tidak ada koneksi ke perangkat
produksi, perubahan dependency, atau penghapusan cache build.

## Perbaikan

- Cargo satu worker, debug info dev dimatikan, release tanpa full LTO dengan 16
  codegen units. Default workspace hanya core + egui; klien lain tetap dapat
  dipilih secara eksplisit.
- Perintah build npm dan CI memakai `scripts/build_guard.py`: satu build aktif,
  pemeriksaan RSS seluruh process group setiap 100 ms, ambang 2048 MiB, cadangan
  RAM tersedia 1536 MiB, serta pemeriksaan limit cgroup v2. Penghentian resource
  menghasilkan exit 75 dan menghentikan process group, bukan aplikasi lain.
- Pilihan broadcast melekat pada sesi, termasuk setelah reorder, close, dan
  reconnect. Command library/scratchpad memakai dispatch target yang sama dengan
  command editor. Koneksi baru yang selesai ketika broadcast aktif tidak ikut
  terpilih secara otomatis.
- Tab aktif tetap menunjuk perangkat yang sama ketika tab lain dipindah/ditutup.
  Payload drag memakai ID sesi, bukan indeks. Tab strip dapat digulir dan Quick
  Connect berada pada bar terpisah.
- Enter command editor mengirim tanpa menambahkan newline; Shift+Enter membuat
  baris baru. CRLF/LF dinormalisasi menjadi CR, command yang gagal dikirim tetap
  tersimpan, dan error write terlihat pada status.
- Quick Connect memvalidasi host/user/port sebelum antre koneksi. IPv6 polos
  tidak lagi dipotong menjadi host + port; bentuk `[IPv6]:port` dinormalisasi.
- Editor mempertahankan SSH key, ProxyJump, folder, tag, catatan, favorit, urutan,
  dan override yang tidak ditampilkan. Save gagal menampilkan pesan dan form tetap
  terbuka.
- Scrollback digambar pada koordinat viewport yang benar; mouse wheel dan
  Shift+PageUp/PageDown tersedia. Seleksi terpisah per sesi. Backspace mengikuti
  preferensi perangkat. Ctrl+C tanpa seleksi mengirim ETX untuk interupsi proses.
- Scratchpad tidak mengambil ruang terminal saat startup; tetap tersedia di View.
  Ada tombol tambah perangkat, status koneksi pending, dan penanda target perintah.
- Registry terminal Tauri mempertahankan decoder saat hibernasi, mengubah decoder
  ketika encoding berganti, membatasi chunk output besar, serta membersihkan output
  dan snapshot ketika sesi ditutup, termasuk event/cleanup terlambat.

## Bukti yang sudah selesai

| Pemeriksaan | Hasil | Puncak RSS sampling |
| --- | --- | --- |
| Core + egui: `cargo test -p opsterm-core -p opsterm-egui --offline --locked -- --test-threads=1` | 47 core + 42 egui lulus | 777,1 MiB |
| Clippy core + egui, seluruh target, `-D warnings` | Exit 0 | 463,9 MiB (716,0 MiB pada pemeriksaan pertama) |
| Frontend `npm test` | 21 tes lulus, satu worker | 325,0 MiB |
| Frontend `tsc --noEmit && vite build` | Exit 0 | 374,4 MiB |
| Playwright `npm run e2e` | 30 alur lulus, satu worker | Build server 397,5 MiB; bukan total browser |
| `python3 -m unittest scripts/test_build_guard.py` | 5 tes lulus | Tidak dihitung sebagai benchmark |

Pengujian regresi awal gagal pada implementasi lama untuk broadcast setelah
reorder, command button yang mengabaikan target, line ending, command tanpa
target, kehilangan konfigurasi perangkat, Backspace, rendering scrollback, serta
empat kasus registry frontend. Tes tambahan untuk event Copy dari toolkit serta klik tab juga gagal sebelum perbaikannya. Klik dan drag-reorder kemudian diverifikasi pada frame input egui yang sebenarnya. Smoke GUI binary dev lama juga membuktikan Ctrl+C
tidak menginterupsi `sleep` sebelum perbaikan.

## Penutupan release

`npm run build:egui -- --offline` **exit 0**. Kompilasi awal seluruh dependency
memakan 17 menit 26 detik dengan puncak RSS sampling **854,5 MiB**. Sesudah
perbaikan GUI terakhir, rebuild final memakan **20,72 detik**, puncak **543,3
MiB**. Tidak ada build yang dihentikan OOM atau watchdog pada pekerjaan ini.

Binary: `target/release/opsterm-egui`, ELF x86_64 stripped, sekitar 16 MB.
Smoke GUI pada binary release final **lulus**, puncak RSS group **213,8 MiB**.
Perintah: `python3 scripts/build_guard.py -- timeout 30s xvfb-run -a -s '-screen 0 1280x850x24' bash scripts/smoke-egui.sh`.

Tes memverifikasi jendela milik PID aplikasi yang baru dibuka, menunggu penanda
kesiapan shell, menginterupsi `sleep` dengan Ctrl+C, mengirim command editor dengan
Enter, membuka dua shell, berpindah tab, dan membuktikan command dikirim ke shell
yang benar melalui file penanda. Screenshot `test-results/egui/desktop.png` dan
`test-results/egui/minimum.png` telah diperiksa secara visual. Ukuran minimum
760×500 tetap menampilkan Quick Connect, tab, terminal, dan command editor.

Percobaan smoke awal menyingkap race fokus jendela, startup shell yang belum
selesai, event Copy toolkit, dan klik yang tertangkap drag wrapper. Harness kini
menunggu jendela terlihat dan penanda shell; kode terminal menangani event Copy,
serta tab memakai satu response yang menerima klik dan drag. Regresi klik/drag dan
Ctrl+C lulus. `cargo fmt --all --check`, validasi sintaks shell, dan kompilasi
sintaks Python juga exit 0.

Jalankan binary dengan `./target/release/opsterm-egui`. Build berikutnya memakai
`npm run build:egui`, atau perintah Python/Cargo pada README.

## Batas verifikasi

Playwright memakai backend IPC simulasi; tes itu memverifikasi frontend, bukan
transport Rust. Tes core memakai fixture dan koneksi lokal yang sudah ada dalam
suite. Smoke GUI memakai Xvfb, software rendering, database sementara, dan shell
lokal; tidak mewakili semua driver GPU atau perangkat jaringan.

GTK native dan paket Tauri desktop penuh tidak dibangun pada pekerjaan ini.
Dependency `russh 0.54.5` mengeluarkan peringatan future incompatibility Rust,
dan Vite memberi peringatan chunk JS melebihi 500 kB; keduanya tidak menggagalkan
pemeriksaan di atas. Tidak ada upgrade dependency spekulatif.

Angka RSS adalah hasil sampling dan menghitung shared pages secara konservatif,
bukan pengukuran puncak presisi. Watchdog mengurangi risiko OOM dan tidak
menjamin perlindungan dari semua lonjakan memori atau proses lain di sistem.
