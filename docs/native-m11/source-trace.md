# Primary source trace (source-only; no platform acceptance)

The local Tauri domain is locked at Tauri 2.11.2 / Wry 0.55.1 /
updater 2.10.1 in src-tauri/Cargo.lock. The installed build-tool package is CLI
2.11.2, agreeing with pnpm-lock.yaml. Saved CLI-tag Cargo.lock names
**tauri-bundler 2.9.2**. No latest template/version was substituted.
All downloaded reference bytes are under upstream/, with SHA256 in source-audit.json.
These are reference sources, not shipped framework dependencies.

## WebKit

Wry 0.55.1 `src/wkwebview/mod.rs:235–245` selects WKWebsiteDataStore default store
for this app (not incognito/no configured identifier). Legacy spec root is
`~/Library/WebKit/com.tezbar.app/WebsiteData/Default/<top>/<client>/LocalStorage/localstorage.sqlite3`.
This lane takes the root explicitly, not from HOME. Alternate WKWebsiteData naming and
older per-origin layouts are not treated as proven matches.

Primary WebKit sources:

- [NetworkStorageManager.cpp](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/NetworkProcess/storage/NetworkStorageManager.cpp): `originDirectoryPath` hashes top/client origins with a salt, creating two levels;
  `originFilePath` uses `origin` at the client-origin directory.
- [StorageUtilities.cpp](https://github.com/WebKit/WebKit/blob/main/Source/WebCore/storage/StorageUtilities.cpp): origin file contains a Persistence-encoded ClientOrigin.
- [WebCorePersistentCoders.cpp](https://github.com/WebKit/WebKit/blob/main/Source/WebCore/platform/WebCorePersistentCoders.cpp): ClientOrigin is top then client; SecurityOriginData is protocol String, host String, optional uint16 port.
- [PersistentCoders.cpp](https://github.com/WebKit/WebKit/blob/main/Source/WTF/wtf/persistence/PersistentCoders.cpp) and [PersistentCoders.h](https://github.com/WebKit/WebKit/blob/main/Source/WTF/wtf/persistence/PersistentCoders.h): String length u32, 8-bit bool, Latin1 or native UTF16; optional uses an engaged bool. Decoder uses native-endian numbers; both shipped targets are little endian. Null strings, explicit ports and trailing bytes are rejected by the limited reader.
- [SQLiteStorageArea.cpp](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/NetworkProcess/storage/SQLiteStorageArea.cpp) and [SQLiteStatement.cpp](https://github.com/WebKit/WebKit/blob/main/Source/WebCore/platform/sql/SQLiteStatement.cpp): ItemTable key TEXT/value BLOB; bindBlob(String) upconverts char16_t and columnBlobAsString reads char16_t. This substantiates UTF16LE on the shipped targets, not arbitrary byte encodings.

Current WebKit main is a captured format reference, not proof of every installed OS WebKit
revision. Disposable binary sidecar + SQLite fixture source is provided. Real disposable
WKWebsiteData O-LS verification remains required. WAL/SHM/journal are unavailable and retryable,
not silently ignored. No immutable URI is used to pretend WAL state was imported.

## WebView2 / Chromium / LevelDB

Locked Tauri `manager/webview.rs:534–550` supplies app-local data to Wry's data_directory.
Wry `webview2/mod.rs:287–345` passes that directory to WebView2 environment creation.
For that explicit user-data folder, the candidate is:
`%LOCALAPPDATA%\com.tezbar.app\Default\Local Storage\leveldb`.
The prose's `%LOCALAPPDATA%\com.tezbar.app\EBWebView\Default\Local Storage\leveldb`
is a separate fallback candidate, not an established result from the locked call path.
The draft accepts an explicit LevelDB directory; it does not guess between them. Parent must
prove automatic discovery on a disposable old app profile before production integration.

- [Chromium 134 LocalStorageImpl](https://github.com/chromium/chromium/blob/134.0.6998.35/components/services/storage/dom_storage/local_storage_impl.cc): storage key prefix is `_` + SerializeForLocalStorage() + NUL.
- [StorageKey](https://github.com/chromium/chromium/blob/134.0.6998.35/third_party/blink/common/storage_key/storage_key.cc): first-party origin serialization has no trailing slash, so exact prefix is `_http://tauri.localhost\0`. HTTPS, other ports, origins and third-party partitioned keys are not interchangeable.
- [CachedStorageArea](https://github.com/chromium/chromium/blob/134.0.6998.35/third_party/blink/renderer/modules/storage/cached_storage_area.cc): encoding tag 0 UTF16, tag 1 Latin1. The same serialization is used in script keys/values. Unknown tags are unavailable.
- [LevelDB 1.23 formats](https://github.com/google/leveldb/tree/1.23/doc): physical log CRC/fragmentation, SST footer magic/index/data blocks. Saved version_edit.cc and write_batch.cc trace manifest tags, table deletion and per-batch sequence increments. Reader follows current manifest and latest sequence/tombstones, not a scan over old file strings. Unsupported Chromium/LevelDB formats fail closed.
- [snap 1.1.1 raw API](https://github.com/BurntSushi/rust-snappy/blob/1.1.1/src/raw.rs): decompress_len inspected before destination allocation; Decoder::decompress into a prebounded output slice. Registry metadata/checksum/license saved, no crate installed.

## NSIS and updater argv

Primary [CLI 2.11.2 NSIS template](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.2/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi)
and nsis/mod.rs are archived. Manufacturer defaults to bundle identifier's second component,
**tezbar**; product is Tezbar; currentUser default is `$LOCALAPPDATA\Tezbar`.
Manufacturer install-location key is `HKCU\Software\tezbar\Tezbar`, uninstall key is
`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Tezbar`. Registry context
uses the upstream SetContext macro. No configured start_menu_folder means
`$SMPROGRAMS\Tezbar.lnk`; restore/update/uninstall shortcut behavior remains upstream.

Local primary `tauri-plugin-updater-2.10.1/src/config.rs:36–43` maps Passive to `/P /R`,
Quiet to `/S /R`. updater.rs:798–816 appends `/UPDATE`, `/ARGS`, then each separately escaped
current argument; empty argv adds no quoted aggregate. Template .onInstSuccess requires `/R`
and forwards `/ARGS` to RunAsUser. Existing portable `StagedWindows::arguments` is the
executable contract and contains the prior missing-/R/empty-argument repairs; it was not edited.
Source fixture text comparison is not NSIS compilation/execution evidence.

## Runtime resources / signing

src-tauri/{tauri.conf.json,tauri.macos.conf.json,tauri.windows.conf.json} are the current
resource-map sources. scripts/build-release.mjs and scripts/lib/{release-policy,
macos-signing,embedding-runtime-policy}.mjs retain all release guards. src-tauri/build.rs
pins embedding archive length/hash to src-tauri/runtime/embedding-macos-arm64.json.
The native planner consumes captured resource hashes and that same catalog; it does not
choose payloads or update versions. Model download caches are excluded.
Core `extension_worker.rs` lazily calls core `bun::locate_bun` when a worker is needed.
The current package resource map does not bundle a standalone Bun executable; the planner
preserves that policy, retaining worker JS/esbuild compatibility files but adding no eager
process or new Bun download. A change to that policy needs separate parent review.

Signing stays with docs/MACOS_SIGNING.md and the existing pin; no signing key/identity store
was accessed. No release inventory was performed because this is source preparation; all
release-guidelines inventory/build/signing gates remain prerequisites to later assembly.
