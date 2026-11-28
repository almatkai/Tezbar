# M5 controller dependencies repaired by the parent

M5's native fixtures exposed two controller defects while its UI was frozen.
The full request and native red logs remain under
`/tmp/tezbar-native-m05-evidence/`, including `model-dependency-request.md` and
`post-checkpoint-normal-tests.log` (83 passed, two failed).

1. Empty-task AI history and ordinary home both used the empty request query and
   shared one `ResultCache`. A chat result could therefore overwrite home and
   ordinary optimistic candidates. The controller now owns a separate bounded
   AI cache. Accepted, current-generation results and pending display select
   their cache by input mode. Current session rows remain the only click
   authority. Both home and AI-history previews survive returning to their mode.
2. An accepted explicit saved pin left a previously deferred Enter alive. Its
   later search reply could start another activation while the pin's argument
   form owned the screen. Successful pin admission now calls `cancel_enter()`
   before creating its normal activation. Rejected pins preserve the pending
   Enter and its normal behavior.

Three permanent controller regressions check those properties, including pending
cache click rejection and rejected-pin behavior. Before the production fixes,
the new mode tests gave **three passed and two failed**, reproducing cache
contamination and the extra activation. After the fixes, the combined model
suite passes **366/366** on both Rust 1.98.1 and Rust 1.85 offline, with no skips.

Logs: `/tmp/tezbar-m5-parent-model-red.log`,
`/tmp/tezbar-m5-parent-model-green.log`,
`/tmp/tezbar-m5-parent-model-msrv.log`.

No public API, renderer, core ranking, store, protocol or release policy changed.
The M5 owner still must rerun its two native repros and frozen final integration
gates. These model results do not accept M5 or the full migration.
