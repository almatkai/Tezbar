# Windows M2/M3 round 2 parent receipt

The linked Windows runner completed the exact immutable `42e22e4b57c5b4c183592e0585e1e251e96adbf9` archive. This does not validate the later GPUI worktree or UI.

Core: 593 passed, zero failed, seven existing ignores after shortening the private fixture temp path. Host: 43 passed. Updater: 26 passed, one existing ignore. Models: 360 passed, one reproduced failure. The library check was not started at the final 3.53 GiB free-space measurement. Earlier failed/cache/path/capacity attempts remain unchanged in the remote evidence.

The model failure expects allow-listed drive-less `file:///outside/icon` to be accepted on Windows. The URL library correctly requires a native Windows absolute file URI; the local fixture now builds its URI from a platform-native absolute path. No production path-policy bypass or test skip was added. The changed fixture has not yet been revalidated on Windows.

Parent downloaded the new 240-member evidence ZIP, 1,440,987 bytes, verified SHA-256 `7b5c75a58d9b941930cd08bdb410b3a098ca6e3baf980bf4c4e3de84ad0c42e3`, and read REPORT/handoff. Local archive: `/tmp/tezbar-windows-m23-42e22e4-evidence-round2-mw4kscqd.zip`; extracted reports `/tmp/tezbar-windows-m23-round2-parent/`. Remote staging and preserved old evidence are described in that report. All 1,836 source hashes and 771 registry records/archives matched there.

No full Windows or full-app/GUI/IME/accessibility/RAM acceptance is claimed. A later exact-source Windows gate and current native application verification remain outstanding.
