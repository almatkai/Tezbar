# Tezbar Color Picker

This is a standalone Tezbar extension repository fixture for
`https://github.com/almatkai/tezbar-color-picker-extension`.

The extension source is deliberately small: the platform-specific screen
sampler belongs to Tezbar, not to the extension. The same repository can
therefore ship unchanged on macOS and Windows.

## What it does

1. Opens the native screen sampler.
2. Lets the user inspect the exact pixel under the pointer.
3. Opens Tezbar's color detail view with HEX, RGB, HSL, HSV, and CSS values.
4. Lets the user copy any format directly from the detail view.

Linux is declared as `coming-soon` until Tezbar's Linux screen-capture bridge is
available.

## Repository contract

- `package.json` is the extension and store manifest.
- `catalog-entry.json` is the small record that can be copied into a curated
  Tezbar catalog repository.
- `src/pick-color.ts` is the source command.
- `.sc-build/pick-color.js` is the checked-in test bundle for immediate local
  testing. Release automation should regenerate it from `src/`.
- `assets/icon.svg` is the store icon.
- `assets/picker-preview.png` and `assets/color-wheel-preview.png` are store
  screenshots.

The eventual release workflow should publish one verified artifact per target:

```text
tezbar-color-picker-0.1.0-windows-x64.tar.gz
tezbar-color-picker-0.1.0-macos-universal.tar.gz
```

## Test it inside Tezbar

From the Tezbar workspace:

```powershell
$env:SUPERCMD_EXTENSION_PATHS = (Resolve-Path examples\tezbar-color-picker-extension).Path
pnpm exec tsx scripts\extension-runtime-harness.ts `
  examples\tezbar-color-picker-extension\package.json pick-color
```

To install it in the running app, open Extensions, paste
`https://github.com/almatkai/tezbar-color-picker-extension`, and choose
**Install Repository**. The repository must be public and contain this
`package.json` at its root.
