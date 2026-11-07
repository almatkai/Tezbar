# Explicit launcher shortcut for the user preview

The preview launched without `--allow-desktop-integration` registers no global
shortcut. The wider opt-in has used Alt+F19; the general preview registry also
rejects production launcher keys. That made the user's expected Alt+Space absent.

`--launcher-shortcut Alt+Space` now explicitly enables only the primary native
tray and its owned key. It does not enable core desktop services, observers,
scanning or command hotkeys. The default preview registry restriction is unchanged.
The existing hotkey event toggles visibility, and the existing successful-quit
cleanup unregisters the primary key. Invalid accelerators fail argument parsing;
an explicitly requested key that fails registration leaves a visible startup error.

The new CLI regression fails before the implementation (unknown argument), then
passes with all 5 CLI tests. An intermediate fixture assertion failed because
macOS canonicalizes /var to /private/var; that failed gate is preserved. The
unchanged default-registry test passes, and the normal debug build passes.

Computer Use could not inspect/quit the installed beta: the full path timed out,
and bundle-ID fallback was ambiguous with existing build copies. The user closed
beta normally. Its main process was then verified absent before launching GPUI.
The real macOS registrar returned success for alt+Space, recorded in the profile
log. The preview window was observed through Computer Use. The separate reopen
command now keeps the explicit shortcut argument. No production binary/profile
was modified; the only sealed bundle is the temporary preview.

The API available for Computer Use sends app-local keys, not physical global
shortcuts. This is OS-registration and live-window evidence, not a claim that an
injected global key, Windows, a release or the full migration was accepted.
