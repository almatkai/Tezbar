# M0–M2 Opus review repairs

Review fixed point: `5597113..7e40b4f`, implementation `80a2aa1`.
The original report is preserved as `docs/reviews/GPUI_M0_M2_OPUS_7e40b4f.md`.
These parent repairs are separate from the concurrently implemented M3 models.
Neither this record nor the original M2 artifact establishes full GPUI parity.

| Finding | Disposition |
| --- | --- |
| F1, macOS requirement argv | The install primitive builds `-R=<requirement>` as one argument. A read-only codesign regression on `/bin/ls` proves an Apple requirement passes and a wrong identifier fails. The pinned requirement and helper Team-ID checks are unchanged. |
| F2, Windows NSIS contract | Passive arguments are `/P /R /UPDATE /ARGS`, followed by individually escaped current arguments. No synthetic empty argument is appended. Escaping follows the locked 2.10.1 plugin, including slash, quotes and trailing backslashes. `ShellExecuteW` consumes the raw command line once, preserves installer elevation behavior and checks its error return. The primitive returns only after launch acceptance; app exit/relaunch coordination remains M11. |
| F3, desktop-bench compilation | `call_backend` receives the Tauri `AppHandle` as an injected parameter. It no longer reads a removed core field. IPC argument shape is unchanged. |
| F4, long preview socket | The lock stays inside the canonical profile. Unix endpoints use a short `/tmp/tezbar-instance-<uid>` directory, checked as a real directory owned by the current user with private permissions. The profile/identity hash scopes its socket. A deep-profile regression binds and activates it. |
| F5, ownership during shutdown | `Instance::begin_shutdown()` refuses new activations explicitly while retaining the lease. Contenders retry ownership for the bounded shutdown interval and can become the replacement owner. Windows contenders release their non-owner named-object handles before retrying. A lost acknowledgement remains an error and never replays an activation whose effect is unknown. M4 must call `begin_shutdown` before draining the core. |
| F6, main-thread effects | Construct `NativeDesktop` on the actual GUI thread. Calls on that thread run inline. Queued calls can be cancelled before execution and cannot produce effects after reporting a queue timeout. If execution already started, the caller observes the operation's actual result; the five-second bound is a queue deadline. Extension tray cleanup drains its resource lock before calling the host. |
| F7, feed-status difference | Intentional stricter native behavior: signed-feed 403/5xx/transport failures surface as an error when no verified update/manual installer is available; 204 permits trying the next endpoint; invalid identity remains an error even without a platform match. This avoids hiding feed/configuration failures. It changes status presentation, never signing, channel selection or install authorization. M11 must exercise these cases in the update rehearsal. |
| F8, private detach alias | `core:terminal:detach` belongs only to the PTY bridge. It is removed from the launcher metadata route. `terminal:native-detach` compatibility and public channel restrictions remain. |
| F9, updater dependency seam | Network/download/staging code and its dependencies are behind the default `native` updater feature. The UI model opts out and can use status, channel, release-note state and URL/policy types without enabling updater I/O. The core dependency still brings the existing core services; this does not claim the entire model dependency tree lacks networking. |

## Validation on the Mac

All checks use the selected worktree; no desktop app or installer was launched.

- `cargo +1.98.1 test --locked -p tezbar-host`: **45 passed**. Six additional
  regressions cover main-thread dispatch, cancelled and started calls, deep profiles,
  retiring-owner takeover and unknown activation outcomes.
- `cargo +1.98.1 test --locked -p tezbar-update native::tests`: **7 passed**,
  including real read-only codesign and the NSIS argument cases.
- `cargo +1.98.1 test --offline -p tezbar-update --no-default-features`:
  **17 passed** at execution time, including the concurrent release-note module.
  This is a feature-isolation check, not acceptance of the unfinished M3 lane.
- The no-default updater dependency tree contains no reqwest, native-tls, tokio,
  tar, flate2 or minisign dependency.
- The existing scoped status-item cleanup and terminal late-notification core
  regressions each passed (**1 + 1**).
- Explicit Rust **1.85** host check and Tauri `desktop-bench` library check passed.
- `python3.11 scripts/native/lock-drift.py` passed. Both lock changes add only a
  direct `url` edge to the updater package; no registry version changed.

Raw logs: `/tmp/tezbar-native-m02-parent-repair-*.log`; lock report:
`/tmp/tezbar-native-m02-parent-repair-lockcheck.json`.

Windows execution/installer rehearsal and fresh Opus confirmation of these repairs
remain pending. The previous clean M2 Windows artifacts do not validate this patch.

## Preserved old behavior

The temporary Tauri adapter still preserves its old extension-tray removal bug and
interrupted Vault-auth blur-suppression behavior. The native status-item implementation
drops the real OS tray. These are recorded for native acceptance, not represented as
new failures caused by extraction. The native adapter must retain cancellation-safe
Vault blur restoration when wired; production Tauri is removed only at M11.
