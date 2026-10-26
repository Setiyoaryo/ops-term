# egui hover, scrolling and memory fixes

Preserve the existing egui layout, palettes, fonts, terminal history and credential security.

1. Reproduce invisible borderless hover, integer-only terminal scroll and oversized font atlas with regression tests.
2. Give borderless controls a distinct hover stroke. Keep native egui window interactions.
3. Preserve fractional terminal scroll in rendering and pointer mapping; clamp history bounds, reset on keyboard navigation/input, and increase wheel speed for saved settings as well as defaults. Keep egui smoothing and event-driven repaint.
4. Cap font atlas width to 2048 (or the smaller driver limit), install fonts before the first frame, and measure RSS/PSS against the existing release under the same workload.
5. Run core/egui tests, lint and real local-shell GUI smoke checks. Inspect screenshots and document actual memory results and limits. Use binaries/reports before required cleanup; leave no build caches.

The 15–30 MiB total RSS target is a measurement goal, not an assumed result: the live process maps roughly 40 MiB of LLVM driver pages alone. Do not relabel private memory as RSS to meet it.
