> Current linked status and superseding evidence: [linked-handoff.md](linked-handoff.md). The original draft below is retained as historical context.

# M10 ledger crosswalk (no rows green)

All new cases below are source-only and unrun. Existing core tests are references, not current execution evidence. This file does not modify the canonical parity ledger.

| Row | Source/cases | Remaining gate |
| --- | --- | --- |
| T-01 | request Create/Boot, both_listeners_before_create_with_metadata_once; existing core PTY creator | parent wire + explicitly isolated real PTY/ConPTY fixture |
| T-02 | replay_startup_deduplicates, replay_preserves_after_snapshot, replay_preserves_unrelated, null_attach_has_no_summary_write, stale_create_and_dispose_detach_without_delete | compile; bounded streaming snapshot + offset/generation seam |
| T-03 | metrics_are_physical_cell_derived_and_bounded, large_unicode_paste_is_atomically_admitted_in_core_sized_chunks; explicit layout_changed | parent after-layout callback; core batch admission for paste failures |
| T-04 | labels_directory_and_separator, labels_use_shell_autocomplete_not_typed_prefix, stale_attach_cwd_and_config_cannot_transfer_selection; command-triggered 80/350/1200ms retries | actual cwd source on both platforms |
| T-05 | finite Kill/Delete/DeleteHistory/PruneHistory; defaults_validate_without_creating_another_settings_owner; existing core retention manager | compile/adapter and core policy fixtures; no duplicate retention/PTY manager |
| T-06 | individual TS case mappings in source-case-map.json, explicit selection, typed Pins, same-id restore | parent UiState/panel/command-bar composition |
| T-07 | vt_split_utf8_csi_erase_alternate_screen, vt_combining_truecolor_selection_and_url, history_cap_retains_viewport_and_palette_edges, vt_device_reply_no_osc52_host_access | compile; GPU/mono fallback shaping; long-output RAM |
| T-08 | key_sequences_app_interception_and_bracketed_paste, sgr_mouse_motion_release_and_modifier_table, gpui_key_interception_uses_terminal_owner | byte-write seam for legacy mouse; platform AltGr/keypad/dead-key acceptance |
| T-09 | zoom_domains_and_escape_sentinel; header buttons 8..24/default12 | parent layout refresh after font/UI zoom |
| T-10 | Alacritty Selection; typed Copy/OpenUrl; gpui_native_ime_commit_copy_url_and_dispose_use_fake_host | parent host drain, native drag edges and URL hit-test/manual |
| T-11 | EntityInputHandler transient preedit; UTF16 range and shaper candidate bounds; GPUI fixture | actual Mac/Windows IME focus/commit/cancel matrix |
| T-12 | data_exit_before_attach_is_not_lost_or_reordered, stale completion/data rejection, bounded_pre_attach_overflow_recovers_from_durable_snapshot | parent existing ordered subscriptions; exact source-generation provenance needs core seam |
| T-13 | public session_rows/select_session/new_session, SessionsToggle effect; existing terminal_sessions panel read only | parent existing panel close/focus/lifetime/placement integration |
| T-14 | dispose requests Detach only; existing core shutdown owner remains canonical | parent confirm quit/shutdown deadline, isolated test and manual running-session quit |
| L-17 / K-16 | Configure/Stop/Pin/Delete key interception; existing M3 defaults, labels, selection and M5 command bar | parent command-bar composition and persistence; no peer edits here |
| S-15 | nonce Payload::Terminal activation; Back sentinel `>`; configure Escape first | parent router/back gate composition |
| S-18 / W-09 | existing parent panel geometry/close handling retained; no second window/process owner | parent panel actions, blur80ms, monitor positioning |
| K-29 / W-06 | zoom_domains_and_escape_sentinel; typed UiZoom separate from terminal font | parent global shortcut ordering and window layout zoom clamp |

Manual/final gates remain Windows, native IME, AX announcements/read-only tree, multi-monitor/DPI, GPU, visual review, keepalive/quit with isolated PTYs, durable-history streaming/RAM and100-cycle retained view behavior. Parent owns fresh Opus review. Full first-party Rust/GPUI migration remains incomplete; Bun/TS is compatibility only, and no new compatibility process was introduced.

## Current linked row evidence (supersedes source-only table)

| Rows | Local evidence | Still required |
| --- | --- | --- |
| T-01/T-03/T-14 | core private managed PTY fixture (explicit nonlogin `/bin/sh -c`, private cwd/profile, only owned PID), binary admission/replay/shutdown; grid metrics | real Windows ConPTY and manual normal app lifecycle |
| T-02/T-12 | production replay registry and GPUI state tests: positions/generation, repeated output, split UTF8/CSI, live gaps, expiry, historical query suppression; byte-chunk bounds and truthful disk failures | long-running real-process recovery/RAM and Windows |
| T-04/T-05 | label/cwd request/policy tests, existing core protocol metadata tests, same native manager | actual platform cwd/keepalive-duration behavior |
| T-06/T-13/L-17/S-15/S-18/W-09 | retained Launcher/CoreEvents transport tests, parent panel actions, pins/back, panel geometry; virtualized full panel list | actual parented window focus/blur/multi-monitor placement |
| T-07 | full UI VT, colours, selection, history-bound fixtures; DECSET1049 cursor semantics repaired without production cursor manipulation | GPU shaping/font fallback, visible cursor/100-cycle/RAM |
| T-08/K-16/K-29/W-06 | key/mouse byte tables, atomic bracketed paste, native dispatch tests, distinct zoom domains | AltGr/keypad/dead keys and platform input |
| T-09/T-10 | production text zoom and selection/copy/URL typed seams; host scheme check | physical mouse selection/URL hit-testing |
| T-11 | native EntityInputHandler synthetic preedit/commit/paste tests | macOS/Windows IME focus/commit/cancel matrix |

Full local commands/results are in ../native-m08/repair-gates.json. These execute
real production helpers with synthetic transport or an explicitly isolated real
PTY, as stated per layer. No full canonical ledger row is marked green; fresh
Opus, platform, manual AX/GPU and RAM remain pending.
