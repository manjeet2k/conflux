# Conflux app icons

Imported from the user-supplied `bird-download-windows-icons-v2` pack on 2026-10-17.
The artwork is the source-matched purple/blue/cyan bird and download arrow with a
transparent background. Default, light, and dark variants in the pack are identical.

- `icon.ico` is the unmodified `win32/bird-download.ico`, containing 14 frames
  (16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, and 256 px).
- `32x32.png` and `128x128@2x.png` are unmodified default 32 and 256 px PNGs.
- `128x128.png` is downsampled from the default 512 px PNG using Pillow LANCZOS;
  the supplied pack has no 128 px export.
- `ui/src/assets/app/` uses the original size-specific PNGs. The 16–24 px assets
  use the pack's simplified interior with the same outer contour.
- `extensions/conflux-browser/icons/` uses the original 16, 32, 48, and 256 px
  PNGs plus the same derived 128 px image as `assets/store/icon-128x128.png`.

Tauri uses these bundle assets for the executable, window, and default tray icon.
Replacing repository assets takes effect when the app is rebuilt and installed.
