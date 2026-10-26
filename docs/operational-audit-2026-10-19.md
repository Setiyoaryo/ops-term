# Audit operasional OpsTerm — 19 Oktober 2026

Bagian audit di bawah mencatat kondisi sebelum perbaikan. Tindak lanjut implementasi
dan hasil verifikasi terbaru dicatat pada bagian akhir dokumen.

Audit kode dengan prioritas egui (klien rekomendasi README), core bersama,
dan lifecycle sesi Tauri. Tidak ada perubahan kode aplikasi atau koneksi ke
perangkat produksi. Ini bukan sertifikasi seluruh fitur atau audit dependency.

## Temuan menurut prioritas

### 1. P1 — Runbook dapat mengirim perintah ke perangkat lain setelah pindah tab

Lokasi: `egui/src/main.rs:1632–1664`.

`Runner` tidak menyimpan identitas sesi target. Setiap `tick_runner()` membaca
output dan mengirim perintah melalui `self.sessions.get(self.active)`.

Pemicu: jalankan script `sleep 3000` lalu `sendline ...` pada perangkat A,
kemudian pilih tab B sebelum tiga detik berlalu. Langkah berikutnya dikirim ke B.
Script login otomatis menggunakan runner yang sama; membuka perangkat lain
dapat pula mengganti runner yang masih berjalan.

Dasar: penelusuran kode, belum diuji pada perangkat nyata.
Perbaikan: ikat runner ke identitas sesi yang stabil sejak mulai; batalkan
ketika sesi target ditutup. Jangan menggunakan indeks tab sebagai identitas.

### 2. P1 — Output tab tersembunyi dapat menghabiskan RAM

Lokasi: `egui/src/main.rs:167–186`, `276–282`, `1025`, `2665–2758`.

Reader memasukkan setiap potongan output ke `std::sync::mpsc::channel()` yang
tidak berbatas. Antrean hanya dikuras lewat `pump()` di `terminal_pane()` untuk
pane yang digambar. Dalam tampilan biasa, tab lain tidak dikuras; split hanya
menggambar dua sesi dan grid maksimal empat.

Pemicu: jalankan output kontinu pada tab A, lalu pindah ke tab B. Data A tetap
terakumulasi selama tab tersebut tersembunyi. Laju 1 MiB/detik berarti sekitar
600 MiB payload dalam sepuluh menit, belum termasuk overhead alokasi. Kembali
ke A juga dapat membuat UI tersendat karena `pump()` menguras tanpa budget.
Deteksi exit dan auto-reconnect tab tersembunyi ikut tertunda karena pemeriksaan
`is_alive()` berada di `pump()`.

Dasar: penelusuran seluruh pemanggilan `pump()`. Sengaja tidak melakukan stress
test atau memenuhi RAM laptop.
Perbaikan: proses output semua sesi dengan budget per frame dan gunakan antrean
berbatas dengan backpressure; pisahkan deteksi lifecycle dari rendering.

### 3. P1 — WaitFor egui melanjutkan script walaupun timeout atau regex invalid

Lokasi: `egui/src/main.rs:1676–1683`.

`if ok || now >= dl` menaikkan indeks langkah untuk kedua keadaan. Regex invalid
juga diubah menjadi `false`, kemudian dianggap selesai setelah timeout.

Pemicu: `waitfor PROMPT_YANG_TIDAK_MUNCUL 100` diikuti `sendline ...`.
Perintah berikutnya tetap dikirim tanpa prompt yang diwajibkan, termasuk saat
login gagal atau perangkat berada pada mode CLI yang salah.

Dasar: penelusuran kode.
Perbaikan: hentikan runner dengan pesan error pada timeout/regex invalid;
lanjutkan hanya setelah pola cocok. Validasi script sebelum memulai.

### 4. P1 — Menutup/reconnect serial di Tauri dapat menggantung

Lokasi: `core/src/providers/serial.rs:196–202`;
`src-tauri/src/session_supervisor/mod.rs:316–321` dan `334–339`.

Serial reader memakai descriptor clone dan blocking read dengan `VMIN=1`,
`VTIME=0`. `kill()` hanya mengubah flag dan menjalankan `tcflush()`, yang tidak
membatalkan read tersebut. Supervisor kemudian melakukan `join()` tanpa timeout.
Saat perangkat diam, close/reconnect tidak selesai. Command Tauri terkait
bersifat sinkron, sehingga jalur ini juga berisiko menahan UI.

Bukti dinamis: probe kecil memanggil provider serial dari library core yang
sudah tersedia menggunakan pseudo-terminal lokal. Reader masih terblokir
300 ms setelah `kill()` dan baru selesai `Ok(1)` setelah data baru ditulis.
Probe tidak memakai port serial fisik. Source serial cocok dengan implementasi
yang diperiksa; probe ini bukan pengujian aplikasi Tauri secara menyeluruh.

Perbaikan: gunakan read yang bisa dibatalkan, misalnya poll dengan cancellation
descriptor; pastikan shutdown membangunkan reader sebelum join.

### 5. P2 — Operasi jaringan tertentu membekukan UI egui

Lokasi: `egui/src/main.rs:782–789`, `860–863`, `1309–1340`;
`core/src/providers/russh_ssh.rs:219`;
`core/src/providers/telnet.rs:322–324`; `core/src/sftp/mod.rs:52`.

Pembukaan native SSH menunggu `ready_rx.recv()` dan pembukaan Telnet memakai
blocking `TcpStream::connect()`, langsung dari callback UI. SFTP mkdir/remove
juga menjalankan proses dan `wait_with_output()` langsung pada UI. Ketika target
lambat, tidak merespons, atau autentikasi menunggu, semua tab dan interaksi UI
tertahan. Sebaliknya, SFTP listing sudah dikerjakan pada thread terpisah.

Dasar: penelusuran jalur callback ke operasi blocking.
Perbaikan: jalankan operasi pada worker, beri timeout/cancellation, dan kirim
hasil serta status pending kembali ke UI.

### 6. P2 — Workspace egui kehilangan referensi sesi SSH tersimpan

Lokasi: `egui/src/main.rs:792–798`, `843`, `1549–1554`.

`open_saved()` untuk SSH memanggil `spawn_ssh()`, yang selalu membuat sesi dengan
`saved_id: None`. `save_workspace()` hanya menyimpan sesi dengan `saved_id`.
Membuka SSH dari device library, menyimpan workspace, lalu restore menghasilkan
workspace tanpa sesi SSH tersebut. Telnet dan serial mengisi saved_id.

Dasar: penelusuran aliran saved session ke serialisasi workspace.
Perbaikan: teruskan ID saved session ketika membuka SSH dari library; pertahankan
None untuk koneksi quick connect yang belum disimpan.

### 7. P2 — Timeout automation Tauri dapat panic pada output Unicode

Lokasi: `src-tauri/src/session_supervisor/mod.rs:303–307`.

Tail dibatasi dengan `text.len().saturating_sub(4096)` lalu `text[start..]`.
Indeks byte tersebut tidak selalu berada di batas karakter UTF-8.

Bukti dinamis: menjalankan ekspresi yang sama pada `"界".repeat(1400)` menghasilkan
panic karena indeks 104 berada di tengah karakter. Ini memverifikasi ekspresi
tail secara terisolasi, bukan keseluruhan command. Dalam aplikasi, pemicunya
adalah pola tidak cocok sampai timeout/exit dengan output multibyte yang sesuai.

Perbaikan: gunakan helper tail UTF-8 yang aman seperti pola dalam
`core/src/text.rs`, atau sesuaikan indeks ke batas karakter sebelum slicing.

## Pemeriksaan dan batas resource

- Frontend: `npm test -- --maxWorkers=1 --minWorkers=1 --no-file-parallelism`:
  **17 tes lulus**, dengan `NODE_OPTIONS=--max-old-space-size=384`.
- TypeScript: `tsc --noEmit`: **exit 0**.
- Core: `cargo test -p opsterm-core --offline -j 1 --lib -- --test-threads=1`:
  **39 tes lulus**; kompilasi incremental selesai sekitar 2,35 detik.
- TypeScript, tes core, dan kompilasi probe dibungkus systemd user service dengan
  `MemoryMax=1G`, `MemorySwapMax=0`, dan `CPUQuota=100%`. Ini membatasi pekerjaan
  audit tersebut, bukan menjamin penggunaan memori aplikasi lain di laptop.
- Probe runtime diberi batas 256 MiB dan timeout lima detik; exit 0, dengan kedua
  kondisi bug serial dan Unicode terkonfirmasi. Source/probe sementara berada di
  `/tmp/opsterm-audit-QIRdTM/`.
- Tidak menjalankan release build, build seluruh workspace, GUI/E2E, koneksi
  jaringan produksi, atau stress test memori. Native GTK belum diaudit menyeluruh.
- Tes yang lulus tidak mencakup skenario temuan di atas. Kode aplikasi belum
  diperbaiki sebagai bagian audit ini.

## Tindak lanjut perbaikan — 19 Oktober 2026

Ketujuh temuan telah ditangani dalam kode:

| Temuan | Perbaikan |
| --- | --- |
| Target runbook | Identitas runtime sesi tetap, terpisah dari urutan/pilihan tab; runner dibatalkan bila target tutup atau reconnect. Beberapa sesi dapat menjalankan login script masing-masing. |
| Antrean output | Antrean egui, Telnet, dan native SSH berbatas dengan backpressure. Semua sesi diproses tanpa bergantung pada pane yang terlihat, dengan budget dan giliran per frame. Drop sesi menghentikan provider. |
| WaitFor gagal | Seluruh regex divalidasi sebelum script mengirim data. Timeout, regex invalid, target terputus dan write gagal menghentikan script dengan pesan. |
| Shutdown serial | I/O nonblocking menunggu port bersama socket pembatalan; kill/Drop membangunkan reader dan writer. Pengujian menggunakan pseudo-terminal lokal. |
| Jaringan memblokir UI | Setup koneksi/reconnect, keyring lookup, dan SFTP listing/mkdir/remove berjalan di worker. Maksimal empat setup aktif, sisanya antre FIFO. Native SSH membatasi seluruh setup, termasuk autentikasi. SFTP memiliki batas 30 detik per proses batch, output berbatas 8 MiB per stream, serta kill process group dan reap pada kegagalan/timeout. File askpass dibersihkan juga pada error. |
| Workspace SSH | Metadata saved device ikut request dan hasil koneksi; ID tersimpan pada sesi dan dapat diserialisasikan. Restore workspace besar mempertahankan seluruh request di antrean. |
| Unicode timeout | Tauri memakai helper tail UTF-8 core yang aman pada batas karakter. |

SFTP menangkap identitas perangkat dan direktori per request; hasil yang sudah tidak
sesuai pilihan saat ini diabaikan. Hasil reconnect untuk tab yang telah ditutup
dibersihkan tanpa membuat sesi baru.

Regresi awal benar-benar gagal sebelum perbaikannya: salah target runbook,
timeout/regex invalid tetap mengirim, output read-ahead tidak berbatas, hilangnya
referensi workspace SSH, reader serial tidak berhenti, panic Unicode, request restore
terbuang setelah 16 pending, serta deadline SFTP dan setup SSH yang tidak mencakup
pekerjaan macet. Review terpisah dilakukan tanpa menjalankan build tambahan.

Pada tahap awal verifikasi perbaikan, kompilasi memakai satu worker,
`MemoryMax=1G`, `MemorySwapMax=0`, dan `CPUQuota=100%`. Dua percobaan yang
melibatkan GTK/Tauri (gabungan dan Tauri terpisah) dihentikan OOM killer pada
batas cgroup 1 GiB, dan GNOME menampilkan notifikasi. Batas tidak dinaikkan.
Setelah pengguna melaporkan notifikasi, kompilasi GTK/Tauri tidak diulang.

### Hasil verifikasi penutupan

- **47 tes core lulus** dan **23 tes egui lulus**, dijalankan ulang dari binary
  tes yang sudah tersedia dari kompilasi komponen yang berhasil sebelumnya.
  Tidak ada kompilasi Cargo pada tahap penutupan ini.
- **17 tes frontend lulus**, dengan satu worker, tanpa paralel antar-file,
  heap Node maksimum 192 MiB dan semi-space 8 MiB.
- **TypeScript `tsc --noEmit` lulus** dan **`cargo fmt --all --check` lulus**.
- **3 tes Unicode terisolasi lulus**: fungsi `wait_timeout` dan tes regresinya
  diekstrak langsung dari source supervisor Tauri, memakai modul `core/src/text.rs`
  yang sebenarnya. Harness menghilangkan kebutuhan memuat Tauri/GTK, sehingga
  hasil ini memverifikasi fungsi tersebut, bukan integrasi seluruh aplikasi Tauri.
- Pemeriksaan penutupan memakai pemantau RSS proses beserta anak-anaknya, ambang
  penghentian 384 MiB, batas waktu 60 detik, dan prioritas CPU rendah. Puncak RSS
  teramati: core 5,2 MiB, egui 13,8 MiB, frontend 325,9 MiB, TypeScript 288,9 MiB,
  kompilasi harness 118,6 MiB, dan pemeriksaan format 46,0 MiB. Angka sampling ini
  bukan pengukuran puncak memori presisi atau jaminan hard limit dari kernel.
- Verifikasi build/test Tauri penuh pada keadaan akhir **belum selesai** karena
  kedua percobaan terkena batas memori. Pemeriksaan itu tidak dianggap lulus.
- Release binary belum dibangun ulang. GUI/E2E, klien GTK native, dan perangkat
  produksi tidak diuji langsung. Tidak ada perubahan dependency atau lockfile.

Perbaikan kode dan tes regresi telah selesai. Hasil tes di atas tidak menjamin
seluruh variasi perangkat, jaringan, atau workflow yang tidak tercakup tes.
