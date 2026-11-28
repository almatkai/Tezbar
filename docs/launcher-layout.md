# Launcher size and scale

Drag any edge or corner of the main launcher to resize it. Hover over the small
dot in its bottom toolbar (or focus/click it) to open **Size & scale**: select an
interface scale from 75% to 200%, enter an exact width and height, or reset size
and scale for that display.
The same controls are in **Settings → General → Launcher appearance**.

- **⌘/Ctrl +/−** changes interface scale; **⌘/Ctrl 0** restores 100%.
- In Terminal, those shortcuts still change terminal text size. Add **Alt** to
  change the whole launcher's scale instead.
- Width and height are logical window pixels, not physical display pixels.
  Interface scale is independent of OS display scaling.
- Size and scale are saved per monitor. Existing per-monitor position storage is
  retained. Reopening restores all three on the active display.
- Moving the launcher to another monitor restores that monitor's size and scale
  after the drag ends, without discarding the drop position.
- Dimensions and position are constrained to the available work area, including
  mixed-DPI displays and changed resolutions. Minimum dimensions grow with scale
  to keep the interface usable; scrolling handles smaller windows.

Native size/scale profiles are atomically stored in `launcher-layouts-v1.json`
beside the active profile's `config.json`. Keeping them separate prevents backend
preference writes from overwriting native resize updates. A new display starts at
760 × 640 and 100%. Reset affects only size and scale on the launcher's current
monitor, not other monitors or its saved position.

The custom edge handles on macOS and Windows track the physical mouse outside the
WebView. macOS does not implement Tao's `drag_resize_window` for borderless windows.
Resizing updates the companion terminal sessions window as well.

## Validation

```sh
pnpm typecheck
pnpm exec vitest run src/shared/windowLayout.test.ts src/renderer/LauncherAppearance.test.ts src/renderer/launcherViewport.test.ts
cd src-tauri && cargo test --locked --lib
```

The launcher shell follows `launcher-viewport-changed` native frames during the
mouse drag, independently of the debounced profile save and WebView `innerHeight`.
Height must not be CSS-animated: that leaves the footer outside the shrinking
native window until the animation catches up. Edge handles are positioned against
the live shell rather than a potentially stale browser viewport.

For the browser regression, with `pnpm dev` running, open
`http://127.0.0.1:5174/__fixtures__/launcher-live-resize.html` and evaluate
`window.runLiveResizeRegression()`. It checks the real launcher shell at 75%,
100%, and 150% while deliberately keeping the browser viewport stale. Every
intermediate frame must keep the footer at the visible bottom, not only the final
frame after release. Expected result: `pass: true`, `failures: []`.

Manual checks: resize from every corner and an edge; hide/reopen; restart the app;
change scale without entering browser zoom; switch between displays with different
DPI; verify independent size/scale and existing position restoration; check terminal
text zoom versus whole-interface zoom. Windows and multi-monitor behavior still
require platform/hardware verification when unavailable locally.
