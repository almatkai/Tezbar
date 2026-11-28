# External executable host

The eleven domain implementations and their assets now live in standalone
extension repositories. Former bundled adapters, shim and inert tests have been
removed; Git history and `NATIVE_V2_QA.md` retain their historical provenance.
The runtime uses one lazily launched process per installed extension/profile.
Catalog, listing, and unrelated search do not launch children. Canonical damaged
packages report repair/upgrade errors and never enter the JavaScript worker.
Legacy aliases may use a covered native command; uncovered legacy commands keep
their existing compatibility path.

Schema 3 uses `runtime: rust`, `execution: process`, `protocolVersion: 1`, and target
ZIPs for Apple Silicon macOS and Windows x64. The first child frame must identify
the selected canonical ID, exact version, and protocol. Protocol-only UTF-8 JSONL
uses LF framing, a 16 MiB cap, separate reader/writer tasks, bounded queues and
pending requests, reverse RPC cancellation, and deadlines. Stderr drains with
constant memory. Requests are never replayed. Session IDs and process generations
fence stale responses/events. Search cancels earlier queries; SDK transactional
state commits enforce latest-query ownership. Busy refresh returns unchanged.
There are at most 128 view sessions; no-view results are disposed too.

The closed typed `host.*` allowlist checks exact method, short method, or group
grants from the trusted manifest. It has no arbitrary core-channel bridge and no
named-port CRUD. Host initialization exposes private package/data/temp paths and
only declared migration files or validated narrow directories. Clipboard, shell, selection, window,
dialog, capture, apps, settings, AI, safety, tray and notification services route
through generic core APIs. Tray/resource IDs are scoped to extension and process;
blur ownership composes across processes and is released on exit. Notification
actions currently return an explicit unsupported error; show/remove are generic.

References include in-flight calls, views, explicit background leases and UI
resources. Zero references trigger a 15 second idle grace, with no idle polling.
Durable `host.background.retain` writes `extensions/background.json` before its
acknowledgement. Startup restores only installed registrations via
`background.activate {key,data}`. Non-durable ownership ends with the process;
durable records survive app/process shutdown until explicit release/uninstall.
The SDK must retain before acknowledging durable domain changes.

A trusted package may declare:

```json
{
  "migrationPaths": {"oldStore": "native-extensions-v2/example/storage.json"},
  "startupProbe": {"key": "restore", "migrationPaths": ["oldStore"], "data": {"migration": true}}
}
```

Migration paths are relative to profile data, explicitly named, and have no
symlink components. Regular files are supported. A directory must be a strict
subdirectory of `native-extensions-v2/<slug>/` or `native-v2-<slug>/`, with a bounded
symlink-free regular-file tree. This permits scoped legacy capture/cache imports
without granting a profile, package registry, or configuration directory. Absolute
paths and traversal are rejected; no whole application configuration is provided. The host never imports stores on
behalf of domain code. On startup, a verified installed package with an existing
referenced file receives the ordinary `background.activate` once per manifest
fingerprint. Success persists in `extensions/startup-probes.json`; failure retries
on a later startup. A successfully restored durable registration with the same key
counts as the probe. No extension-name startup branches exist.

Views declare `refreshMs: null` for zero polling or an interval of at least 500 ms.
`session.invalidated` events carry sessionId/revision; the host adds generation.
The renderer rejects stale generations/revisions, coalesces refreshes, and suspends
polling while hidden. Absent metadata preserves legacy 500 ms refresh behavior.

Graceful shutdown permits SDK disposal and reverse responses before fencing and
killing the entire process tree. POSIX groups cover helper children. Windows starts
suspended, assigns a kill-on-close Job Object, resumes, and uses no-console flags.
Windows code has not been executed on this macOS host. Abrupt host-death cleanup is exercised with the actual independent supervisor
and helper processes on macOS. Windows containment and real platform UI behavior
still require native release-host verification. Process grants are not an OS sandbox.

The trusted catalog pins ten public dual-target releases and one verified private
release. All eleven packages are separately published.
Private acquisition uses generic local import; verified release coverage does not
prove desktop/platform acceptance. Run isolated platform smoke tests after pinning.
Keep resources,
notices and all extracted feature tests in their standalone repositories. Generic
JS compatibility sampler helpers remain in the app; extracted audio/timer assets
are no longer bundled or provisioned by app builds.

Legacy JavaScript timer notifications retain their compatibility delegate and
activation behavior. It initializes on its first worker notification, with no
extension-specific startup hook. External executables use the separate generic
notification API, which requests permission on first explicit delivery.

For a combined release manifest, an omitted or empty trusted `files` object
delegates the per-target inventory to the ZIP whose complete SHA256 has already been verified.
Every member must be inventoried, executable digest must match the target pin,
and all remaining identity/policy fields match exactly. The active manifest stores
the resolved inventory, and its own digest is pinned in `active.json`. Nonempty
trusted inventories must match exactly. The public catalog remains parent-owned.

Acquisition rules live in `acquisition.json`, separately from the exact hash-bound
release manifests. An absent rule means ordinary remote acquisition; explicit
`remote` and `local-import-only` are supported. Unknown/malformed rules disable
remote installation. Declared replacement IDs and the same repository reference
inherit the rule, so legacy catalog rows cannot offer a misleading download.
Installed legacy command fallback remains available. Local-only catalog entries
preserve legacy fallback until a validated package is installed, then expose normal command coverage. Both the
installer and app entry point reject remote install/reinstall with local-import
guidance. Explicit local ZIP import retains every package validation check. No
authenticated HTTP integration or GUI import is provided. Close and reopen Tezbar
after CLI import: it does not notify an already running UI or launcher.

Canonical uninstall retains host preferences separately at
`<profile-data>/extension-preferences/<canonical-id>.json`, including extension
and per-command values and unknown keys. Reimport uses package-owned preferences
first, then retained preferences, then the existing legacy preference fallback;
saves update the selected file. Before removal the selected package-owned or
legacy fallback object is retained; a current package-owned file replaces any
older retained copy. The retained file becomes the current canonical preference
store after reimport, independently of the package's managed install location.
Legacy files and extension-private domain stores remain in place.
Unreadable/malformed preferences or a failed retention write abort removal before
deleting original package files. Preferences and package
metadata mutations share the install/uninstall locks. Uninstall still removes
all managed executable/version/activation/discovery state, durable background
and startup-probe registrations, installed records, and launcher index ownership;
the retained preference directory is outside package discovery.
