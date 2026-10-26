# OpsTerm native paste fallback

Source: the published egui-winit 0.35.0 crate, crates.io checksum
`9ea6bf3608db949588b95b8b341ee358d0c3f95cf4dc3f53d8d76717edee87db`.
Upstream: https://github.com/emilk/egui/tree/0.35.0/crates/egui-winit
Licensing: upstream MIT OR Apache-2.0; both license texts are retained here.

One behavioral patch in src/lib.rs: only consume a native paste shortcut when
nonempty clipboard text produced an egui Paste event. Otherwise retain the
original key event. OpsTerm encodes the terminal's Ctrl+V so CLI agents can read
the image clipboard themselves. No image data is converted or stored by this
patch, and text paste stays native.

Keep this patch narrow; compare/remove it when upgrading egui-winit if upstream
supports equivalent fallback or a raw clipboard-shortcut event. The normalized
published Cargo.toml is retained for standalone dependency resolution; its
license include paths are adjusted to the local license files.

Published source commit: `6f15dc0e16b26edce1fc2a05212eaf7e749c1d05`.
