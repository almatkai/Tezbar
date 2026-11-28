# External Rust extension CLI (schema 3, protocol 1)

The Tezbar executable handles these arguments before the desktop GUI starts:

```sh
tezbar --extensions-v2-list
tezbar --extensions-v2-list --json
tezbar --extensions-v2-install com.tezbar.native.perplexity
tezbar --extensions-v2-install --all
tezbar --extensions-v3-import /absolute/trusted-release-manifest.json /absolute/package.zip
```

The v2 flags remain compatible names. Listing is offline: it reads trusted catalog
metadata and verifies installed files without a runtime, network request, Bun, or
extension process. `status` describes verified release availability for this
platform. `installationStatus` is `installed`, `not-installed`, or
`needs-upgrade-or-repair`. Schema-2 package/native-v2 marker files never count as
executable installations. No release pins are invented: unpublished entries remain
unavailable, and compatible legacy packages remain discoverable.

Install prints `operation: "install"` and a `results` array with per-ID `ok` and
`result` or `error`. `--all` attempts each ID independently and exits nonzero if any
fail. Invalid arguments exit 2. Installation downloads bounded credential-free
HTTPS ZIPs through the same installer used by the desktop app. It verifies archive
SHA256, packaged manifest identity/policy, executable SHA256, and every asset in
`files`. No Cargo/npm/Bun build runs on the user's machine.

Local-import-only catalog entries reject ordinary Install and reinstall, including
`--extensions-v2-install`; unknown/malformed acquisition rules also disable remote
installation. Acquisition metadata is host catalog routing, outside the release
manifest policy. Installed local packages keep their normal command coverage.

For a private package, obtain its ZIP and trusted release manifest through your
own authenticated channel, verify the publisher, then use local import. Import
requires no embedded GitHub token or public mirror. The trusted manifest contains
real target artifact hashes; `artifacts` can be absent/empty inside the ZIP to
avoid a self-referential archive digest. A combined trusted release manifest can
omit `files` or use `files: {}`: after the target ZIP's trusted SHA256 passes, the
installer derives its complete inventory from the archive manifest and verifies every file plus
the separately trusted executable digest. A nonempty trusted inventory must match
exactly; explicit null, strings and arrays are rejected. The resolved target
inventory is pinned in the active manifest for later tamper checks. All other identity, command, and policy fields must match. Importing a
publisher's executable authorizes native code execution; host capability grants
constrain Tezbar services and are not an operating-system sandbox.

Close and reopen Tezbar after CLI import so installed discovery, startup and launcher
activation run predictably. Import does not emit an event to a running UI. Use the
same local import command for private package updates or repair.

Packages use `extensions/packages/<canonical>/versions/<version>-<uuid>/`.
`active.json` atomically selects the version and pins its manifest digest. Invalid
ZIPs, hash mismatches, traversal/drive/UNC paths, symlinks, special files, duplicate
case-folded paths, excessive files, and archive expansion limits fail before
activation. A failed activation restores the previous installed database and keeps
the previous version. Root preferences, extension-private data, and legacy stores
survive upgrades. Explicit uninstall removes only the selected package and its
background registrations; extension data remains available for migration.

Defaults: macOS `~/Library/Application Support/com.tezbar.app`; Windows
`%LOCALAPPDATA%/com.tezbar.app` (fallback `%USERPROFILE%/AppData/Local/com.tezbar.app`);
Linux `$XDG_DATA_HOME/com.tezbar.app` (fallback `~/.local/share/com.tezbar.app`).
`TEZBAR_DATA_DIR` overrides data; `TEZBAR_CONFIG_DIR` overrides `~/.openray` config.
Linux has no published executable target in this contract. Listing never reads
saved preference values or credentials.

See [EXTERNAL_RUST_EXTENSIONS.md](EXTERNAL_RUST_EXTENSIONS.md) for lifecycle and
migration policy and [schema 3](../schemas/extension-v3.schema.json) for packages.
