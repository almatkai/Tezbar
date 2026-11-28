# Parent check of the M3A repairs

Implementation commit: `42e22e4b57c5b4c183592e0585e1e251e96adbf9`.
The parent verified all 25 delivered source/fixture hashes and all 30 regression
filters against the final model/instant logs. All 82 tracked TS test-source
hashes remain unchanged. The original Opus report is byte-identical.
The review dispositions and interface changes are documented in
[repair evidence](launcher-review-repair-evidence.md); fresh independent Opus
closure remains pending for M3A and M3B.

The repair lane's full parallel core run failed five timing/process tests, each
of which then passed in isolation. The parent reran the **whole** unchanged core
suite with two test threads, rather than treating isolated success as a passing
full run:

```sh
CARGO_BUILD_JOBS=2 cargo +1.98.1 test --offline --locked -p tezbar-core \
  --features native-ui-instant -- --test-threads=2
```

Result: **657 passed, zero failed, eight existing ignores**, in 159.64 seconds.
Log: `/tmp/tezbar-native-m03a-parent-core-2threads.log`. No timeout assertion,
test body, ignore or production source was changed for the rerun. The earlier
failed run is preserved in the lane evidence. This establishes a passing full
command under the stated concurrency; it does not prove the high-concurrency
failures can never recur.

An immutable Git archive of the implementation was verified against all Git
blob identities on the Mac, then transferred to the linked Windows machine as
build input. It contains **1,836 files**, no symlinks or Git metadata, and none
of the concurrent uncommitted M4/M5 repairs.

- Archive SHA-256: `589e5f41cb9ce9bcfe7045b4ef4b83d2305333716c2ca25d03c358771a0a23e5`.
- Manifest SHA-256: `d6974d101fc0e501bfe14f1024da954927c7762b25c1050ca1771dddefc02512`.
- Mac evidence: `/tmp/tezbar-native-m3-parent-42e22e4-evidence/`.

Windows execution is a separate pending gate. The linked machine is
`DESKTOP-OB83S8J`, environment `e8399889-5c93-4fa0-b469-36ba9262ff6b`.
The Desktop release checkout must remain untouched. Native command-bar,
foundation, visual/OS and full-app RAM acceptance are not inferred from these
model/core results.
