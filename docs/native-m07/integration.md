# M7 native Settings and onboarding integration

Worktree `Tezbar-native-rust-agent`, branch `feat/native-gpui-app`. M7 remains unstaged/uncommitted. M7 now owns App/Launcher composition within the explicitly released scope. Parent owns concurrent M6 repairs, shared models/core/host/UI, manifests, reviews and validation coordination. M8/M9 remain unexported during the final M6/M7 compiled-input freeze.

## One durable write owner

App constructs one `SettingsStore` after its existing `CoreClient`, injecting clones into Settings, onboarding and the launcher model picker. There is no additional runtime or config store. `SettingsTransport::durable_config()` reads the existing `CoreContext.config` in the blocking pool. Projected `llm-config-get` is presentation data, never the queue's write merge base.

```rust
SettingsStore::new(Arc<dyn SettingsTransport>)
SettingsStore::owner() -> OwnerId
SettingsStore::enqueue(owner, Vec<Edit>) -> u64
SettingsStore::discover(owner, Edit)
SettingsStore::validation(owner, key, Option<&'static str>)
SettingsStore::retry()
SettingsStore::drain() -> HostFuture<()>
SettingsStore::drain_owner(owner) -> HostFuture<()>
SettingsStore::drain_with(EffectBarrier) -> HostFuture<()>
SettingsStore::admit(SettingsRequest) -> Result<u64, String>
SettingsStore::operation_result(id).await -> Result<Value, String>
```

Synchronous admission assigns production M3 revisions before navigation. A retained Tokio task finishes fresh reads/writes even if a GPUI waiter is cancelled. Close/quit use the all-owner barrier. Picker acknowledgement waits for config intents admitted before its drain to settle (unrelated owned downloads do not delay that settlement), then checks `WriteQueue::owner_failure_count(owner)`. A peer failure neither acknowledges a pending picker write nor marks a successful picker write failed. `model_saving` still gates submit; results preserve N1's independent launcher-state error.

The finite request enum has redacted Debug and no arbitrary channel variant. Other-store operations have owned tickets and can remain concurrent, allowing per-model cancellation to finish independently of a held start. Interrupt admission reserves one of sixteen slots. Operational failures (auth/browser/test/download) are not replayed by Retry saved changes. A shared config lane serializes credential-persisting device polls against fresh-read/config-save transactions.

Aliases expand to fresh `commandAliases` under this executor. All unsaved aliases are retained together, including a failed alias followed by a peer save. **Ownership is coarser than per-alias M3 revision ownership.** No duplicate model helper or stronger ownership claim is made; review must retain this limitation.

## Native surface and effects

```rust
SettingsContent::new(store, &UiState, window, cx)
SettingsContent::attach_events(&CoreEvents, cx)
SettingsContent::activate(window, cx)
SettingsContent::activate_tab(Tab, window, cx)
SettingsContent::navigate(&Navigation, window, cx)
SettingsContent::review_project(cwd: String, nonce: String, window, cx)
SettingsContent::set_visible(bool, cx)
SettingsContent::set_ui_state(&UiState, cx)
SettingsContent::set_update(AppUpdateStatus, channel: String, cx)
SettingsContent::set_layout(display: String, Profile, window, cx)
SettingsContent::parent_effect_finished(Result<(), ()>, cx)
```

Constructors/render do no I/O. Activation loads the active surface. Retained tabs are General, AI, Voice, Knowledge, Extensions, Permissions, Storage and Advanced. Shared controls/TextInput provide input/password/keyboard semantics. Tabs use roving focus, arrows and selected roles. Confirmations restore focus. Dirty input survives late initial reads and silent canonical hydration.

Stable `SettingsEffect` variants: `RetryPersistence`, `Theme(Theme)`, `RememberTab(String)`, `Currency(Option<String>)`, `AutoUpdate(bool)`, `UpdateChannel(String)`, `Update(UpdateAction)`, `ReleaseNotes`, `LauncherLayout { width: Option<f64>, height: Option<f64>, zoom: Option<f64>, reset: bool }`, `BrowseStore`, `Quit`, `Close`, `OpenPermissions`, `LegacyCacheCleanup`.

App subscribes once before activation. Mailbox effects retain original entity identity; async feedback resolves only that retained content/window. `EffectBarrier` keeps pending parent writes and the latest failure per key; stale successful feedback cannot erase a newer failure. UI prefs use Launcher's existing atomic UiState owner. Corrupt initial files are preserved with a visible error. Layout reuses the display-specific writer. Release channel uses the updater's existing channel file and UI-state check timestamp. Update actions go to typed UpdateInbox; no install success is fabricated. Legacy cleanup refuses deletion until M11 has a durable import receipt. Quit invokes the existing core confirmation after shared drains.

## Lifecycle and project handoff

The normal native Settings window is 920×680. `set_content` retains the child and forwards Close exactly once. App adds the store/parent-effect composite drain. Existing immediate close acknowledgement, finalize generation, error retention and unacknowledged-only three-second fallback remain shell-owned. App restores the launcher before destroying Settings. Failed saves keep the same window/content open; Retry uses the retained owner. `Settings::close_failed(generation, error, cx)` accepts feedback only for its own close generation, so stale errors cannot poison a retry or a newly created shell. The composite drain rechecks config intents admitted while parent UI-state persistence was finishing.

Live Settings intents are delivered directly to the content after Launcher acknowledges the inbox delivery. They never replace Launcher.navigation. Closing Knowledge Settings from Indexing therefore resumes the same Indexing entity. Explicit destinations (Store/Permissions/notes) run after successful Settings close.

M8 seam: `Owner::settings_project_review(cwd: String, nonce: String, cx)` retains a handoff through boot gating and calls `SettingsContent::review_project`. It preserves the source main surface, selects AI, and invokes only the existing review channel. Duplicate current UI nonces coalesce. Trust/dismiss use the core-issued `projectReviewId` and reviewed folder/hash, not that UI delivery nonce. Review does not test, start, enable or trust. `Owner::settings_project_review` is currently unused until M8 dispatch wiring; no M8 dispatch/export integration or end-to-end chat notice acceptance is claimed by this seam.

```rust
Onboarding::new(store, &UiState, macos: bool, window, cx)
Onboarding::activate(window, cx)
Onboarding::go(step, window, cx)
Onboarding::set_visible(bool, cx)
Onboarding::finish(cx)
Onboarding::retry_finish(cx)
onboarding::boot_gate(store) -> HostFuture<bool>
enum OnboardingEffect { Finished, Settings(SettingsEffect) }
```

App gates only after successful coreReady, before first requested show. Raw config gets one 400ms retry then fails open. Mounted onboarding survives hide/show and late replies. Windows omits macOS permissions. Shortcut conflicts preserve the old binding. Practice uses production input modes/calculator and finite path-completion/clipboard channels; it does not launch files/apps/terminal/agents. Provider/Knowledge steps reuse retained Settings content and the shared queue. Finish/Skip write hasCompletedOnboarding and emit Finished only after durable success; errors remain retryable.

## Resolved seams and evidence limits

[parent-contracts.md](parent-contracts.md) records parent-owned canonical agents, targeted contribution patches, opaque SecretSlot, task-owned Edit::TaskSelection and per-model Voice cancellation. The view never infers raw secret refs from labels. Dirty secret fields keep their original opaque slot; after manifest reload an obsolete slot is rejected until explicit Reload credential fields.

Voice activation reads the existing `voice:stt:modes` channel. Every listed model can be selected before installation, with an explicit selected radio state. Native controls retain partial-download cleanup and per-model cancellation; selecting a model does not start downloading it.

The three U3 dispositions remain: Startup read-only checked; PushToTalk descriptive; Last Screen saved timeout still inert in launcher lifecycle. No new OS capability or live behavior is claimed.

[source-case-map.json](source-case-map.json) retains exact TS/import hashes, reference case names and native fixture names. All source rows remain partial. M3 controller evidence does not substitute for native acceptance. GPUI fixtures mount production entities with synthetic transport. See [verification.md](verification.md) for actual logs. Physical keyboard/IME/screen reader, platform windows/dialogs, real providers/credentials/downloads, updater installation, Windows, RAM, fresh M7 Opus acceptance and M8–M11 completion remain unclaimed.

## 2026-11-07 retry handoff

The previous usage-limited child was read for ownership/details only and was not restarted. No subagent/provider/account fallback was used. XERJ received one bounded unavailable-server check, then exact local source searches continued without recovery/indexing.

The final [compiled-input freeze](source-freeze-final.json) records all 309 conservative workspace inputs, cross-directory include/build assets, Cargo lock/toolchain, the HEAD/branch and 25 M7-owned/coowned file hashes (including three specifically authorized test-only files). Inputs include the parent M6/model/shared-UI integration work. M6 TaskIndicator hooks were preserved; whole-file formatting of command-bar mod/view used `skip_children=true`. The three old launcher fixtures were updated only under explicit parent/user release; their new assertions and retained failures are listed in [verification](verification.md).

All edits remain unstaged/uncommitted. Parent retains integration/commit and the mandatory fresh Claude Opus 5.5 review. Automated passing results do not accept M7 or close ST/O/C rows; M8–M11, platform/manual and RAM acceptance remain separate.
