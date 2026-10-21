# Native V2 compiled CLI

The Tezbar executable accepts these headless arguments before Tauri starts:

```sh
tezbar --extensions-v2-list
tezbar --extensions-v2-list --json
tezbar --extensions-v2-install com.tezbar.native.timers
tezbar --extensions-v2-install --all
```

`--extensions-v2-list --json` prints an array of the bundled manifests. Each command has its computed `runtime` and, for unavailable commands, a `gap`. `status` summarizes compiled coverage on the current platform; `verification` records platform checks, including `not-run`. `installationStatus` is `installed`, `not-installed`, or `invalid`. `installed` is true only when both the package and native marker have matching schema, canonical ID, Rust runtime, and bundled version. The plain list prints ID, runtime, covered/total commands, coverage status, installation state, declared platforms, and verification states.

Installation prints one JSON object with `operation: "install"` and a `results` array. Each result contains an ID, `ok`, and either the installed manifest under `result` or an `error`. `--all` attempts every bundled canonical ID independently. It can partially succeed; errors cause a nonzero exit code. This operation writes under the configured Tezbar data directory and does not start Bun or the Tauri GUI. An unknown CLI switch is left to normal app startup; malformed Native V2 arguments exit with code 2.

Data path defaults: macOS `~/Library/Application Support/com.tezbar.app`; Windows `%LOCALAPPDATA%/com.tezbar.app` (or `%USERPROFILE%/AppData/Local/com.tezbar.app`); Linux `$XDG_DATA_HOME/com.tezbar.app` (or `~/.local/share/com.tezbar.app`). `TEZBAR_DATA_DIR` overrides this path. `TEZBAR_CONFIG_DIR` overrides the default `~/.openray` config directory. CLI listing reads only bundled manifests and installed package/marker metadata. It does not read saved preferences or credentials.

The entry point is `native_v2_cli::dispatch() -> Option<i32>`. The application entry point must call it after child supervisor dispatch and before `app_lib::run()`, then exit with its returned code. The library must expose that call to `main.rs` (for example, `app_lib::dispatch_native_v2_cli()`).
