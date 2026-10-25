# Native V2 extensions

Native V2 is manifest schema version 2, not an application version. A bundled extension has a canonical `com.tezbar.native.<slug>` ID and lists the `raycast.<slug>` ID it replaces. The Rust binary contains the adapter dispatch; JSON cannot add executable code.

`src-tauri/src/native_core/extensions/v2_catalog.json` contains public package metadata and command names from the eleven installed extension sources. It contains no saved preferences, storage, credentials, or user data. `schemas/extension-v2.schema.json` defines the format. At runtime, `v2_catalog` compares each command with compiled dispatch and the declared platform. It reports `rust` only for a compiled handler on that platform; otherwise it reports `unavailable` with a gap. Status is derived from those commands. Verification is separate and starts `not-run` on both platforms until a real QA artifact records the outcome.

The store also retains 98 own V1 catalog entries covering 247 commands. These are JavaScript compatibility extensions; they are not included in Native V2 coverage. Seven entries come from local public package manifests and use their declared `com.tezbar.*` IDs, including `com.tezbar.google-translate` whose package name is `translate` and `com.tezbar.visual-studio-code`. A partial Native V2 replacement appears before its V1 counterpart so remaining V1 commands can still be found.

Installing a canonical ID writes its bundled manifest and `native-v2.json` marker under the managed `extensions/packages/<id>` root, plus an installed database entry. The marker alone does not supply execution: the native session also requires compiled dispatch and an installed command. The Raycast package remains in place for reversible fallback. Existing preference data stays in the legacy package and is read when the native package has no preference file. Native adapter storage has its own namespace; no conversion of unknown legacy storage is asserted.

`scripts/extensions-v2-list.mjs` calls the compiled Rust CLI to list actual compiled coverage and installation state. Set `TEZBAR_NATIVE_CORE_CLI` to a binary path when needed. A missing binary is an error; the script never reports static JSON as working code.

Windows and macOS support are independent of QA. Native V2 command platform declarations describe intended compiled replacements on both platforms, even when the original Raycast manifest was macOS only. Verification remains `not-run` until tested. OS integration commands can still fail with a concrete capability error when an external app or helper is absent.
