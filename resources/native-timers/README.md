# Native timer sounds

These six WAV files are the alert assets from the installed Raycast Timers extension,
vendored unchanged so Native V2 works without a legacy extension installation.
They are covered by the Raycast extensions MIT license shipped in
`licenses/raycast-extensions-MIT.txt`.

Tauri bundles this directory as `resources/native-timers`. Both development and
packaged runtimes resolve it from `CorePaths.resource_dir`. The Rust clean-profile
regression checks every selectable WAV and starts a timer with the default sound.
