# Parent check of M4 repairs and shared M5 integration

The parent verified all **25 M4 source hashes**, all **14 proof-artifact hashes**
and all **35 regression filters** against the repair lane's final logs. The
original Opus report and historical foundation map are preserved. All three
native fixture scenarios report success, zero owned surviving PIDs and removed
endpoints. The held core-stop scenario records 8.051998292 seconds; these are
dirty-source lifecycle facts, not a visual or full-product acceptance claim.

The compiled app includes the shared, in-progress M5 command-bar module. Its
eleven-file source map agrees with the M4 control fixture snapshot, and the
normal binary receipts agree. The parent freezes these supporting files in a
coherent integration checkpoint; this does **not** accept M5 or erase its known
remaining work in `docs/native-m05/command-bar.md`.

M5 recorded a failed full bench-control run during the same interval in which
M4's negative test temporarily removed the worker-safe event bridge. File times:
the negative-test log spans 06:18:39–06:18:54 UTC; M5's failed run starts at
06:18:40. This overlap supports a build-race explanation but does not establish
the exact source bytes compiled into that failed binary. Both failed logs stay
preserved and are not relabelled as passes.

The parent therefore reran the complete integrated bench-control/UI suite on
the current frozen source, checking **35 source hashes before and after**:

```sh
CARGO_BUILD_JOBS=2 cargo +1.98.1 test --locked -p tezbar-native -p tezbar-ui \
  --features runtime-shaders,bench-control -- --test-threads=2
```

Result: **57 app + 37 UI passed**, zero failures/ignores, 7.53 seconds. No source
hash changed, no timeout occurred, and no source was modified for this rerun.
Evidence: `/tmp/tezbar-native-m04-parent-repair-evidence/`, including
`source-before.json`, `source-after.json`, `test-result.json` and
`full-bench-ui.log`. This is the stated command's result, not a claim that every
possible test/build scheduling interleaving has been proven.

Fresh independent Opus review remains required. Manual visual, OS IME,
VoiceOver/Narrator, physical tray/hotkey/snap/resize, multi-monitor/Windows GUI
and full-app RAM acceptance remain open. No installed product, production
profile, signing identity or release default was changed.
