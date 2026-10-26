# Terminal font and text palette implementation plan

Goal: add persisted terminal font family/weight, softer ANSI color palettes, text brightness and a live preview inside the existing egui Settings. Preserve shell data, layout, UI theme, scroll behavior and the small font atlas.

Architecture: a focused `egui/src/appearance.rs` owns typed appearance settings, known installed monospace font discovery, palette resolution and the Settings controls/preview. UiSettings contains an appearance value with backward-compatible defaults. Only the chosen terminal font and existing UI sans font are loaded. Terminal Paint receives resolved color settings each frame; ANSI 16-color names/indexes follow the palette, and truecolor/256-color applications retain their semantic color with optional foreground brightness applied.

- [x] Add failing persistence/legacy-settings regressions, then run guarded egui release tests with a temporary XDG_DATA_HOME. Avoid core vault tests and real OS keyring mutations.
- [x] Implement four text palettes (Original, Soft, Warm, Cool), brightness 55–100%, and installed font options (System, JetBrains Regular/Medium, Ubuntu Mono, DejaVu, Liberation, bundled). Missing/unknown options fall back safely. Add unit coverage for colors, brightness, fallback and font discovery.
- [x] Integrate immediate Settings controls and preview, reload fonts only after actual selection changes, persist choices, and ensure the enlarged settings body can scroll at minimum window size.
- [x] Verify all egui tests, Clippy and release build. Review independently. Real GUI test selects font/palette/brightness, checks output changes and settings persistence after restart; inspect screenshots and exercise local shell smoke.
- [x] Install verified binary atomically with rollback, leave the active shell process alone, document settings and clean all build/test caches under the build lock.
