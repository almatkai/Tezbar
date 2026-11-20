# Independent Linear port review

Reviewed 2026-11-20. Read-only inspection of the standalone Rust implementation, pinned upstream sources, SDK/host contracts, package helper and real-process fixture harness. No build, executable, real Linear request, account change or desktop action was performed. This report is the only file written by the reviewer.

The findings below refer to the original frozen source archive `linear-frozen-source.zip` (SHA-256 `2e54164dff020afbffd98e30adfd084df074997cd2f0662a717a45fa877047e9`). The parent has begun repairing both findings in the live source. Those repairs need fresh tests, package hashes and a new freeze; this report does not sign off changed files using old evidence.

## Findings

### P2 — inbox callbacks can target a new menu after disable/re-enable

In the frozen `src/tray.rs:58`, a start after disable initializes revision to zero. Publication emits row actions as `open-{revision}-{index}` and `read-{revision}-{index}` (`:151-152`). Disable sets the stored inbox to `None` (`:192`); the next first publication therefore reuses IDs such as `read-1-0`. Event handling validates revision and index but not the inbox generation (`:218-229`). `NEXT_GENERATION` fences the polling task, but that identity is absent from callback IDs.

Concrete trigger: capture a first-menu notification callback, disable the menu, then start a new menu whose first entry is a different notification. Deliver the old `read-1-0` event. It passes the new revision check and marks the new first notification read. A different personal key/workspace can also be selected when restarting. The unversioned global `read-all`, `refresh`, `open-inbox`, `configure` and `disable` IDs are similarly accepted against whichever inbox currently exists.

Include generation and render revision in every actionable callback ID, and validate both before any effect. A synthetic disable/re-enable test should retain an old row and global action, replace the inbox data/key, and prove the old callbacks produce no API or host effects. Same-generation polling and Hide are otherwise serialized by the inbox mutex; I did not find the unlocked late-publication race seen in the separate Brew port.

### P2 — changing Team or Project resets a selected issue priority

In frozen `src/main.rs:324`, a dependent field callback applies the complete `formValues` object to the form. Native GPUI dropdown values are strings, and `Form::apply` retains them unchanged. Frozen `src/view.rs:714-717` renders priority using only `as_i64().unwrap_or_default()`, then converts that number back to a string. The submit handler already accepts numeric strings (`src/forms.rs:146-149`), so the render and submit paths disagree.

Concrete trigger: select Urgent (`"1"`) while creating/editing an issue, then change the Team or Project. The callback retains `priority: "1"` in state, but rerender emits `priority: "0"`; the UI resets to No Priority and a subsequent submit can remove the intended urgency. This can also happen after a validation-error rerender when the submitted priority was a string.

Render numeric strings using the same bounded parser as submission. Cover the actual dependent-field callback and a validation-error rerender with a nonzero priority, asserting the emitted dropdown value and eventual mutation input.

## Evidence independently checked

- The recorded Windows ZIP SHA-256 is `4d1d48375d610338d0d103839e8dba512ed75712ac5945eda8b56de58d08bee2`. I read the archive and verified all 579 member names, exact nonempty inventory, every member SHA-256, ZIP CRCs, uniqueness and case-folded uniqueness. The packaged policy matches the supplied native manifest apart from `files`/`artifacts`.
- The packaged executable SHA-256 is `43de382c670dfe296e0381e8f1164a2ab56506a5f9a5c8f84a32481da336efcf`. Its bytes do not contain the fixture endpoint marker. No binary was executed by this review.
- All 486 upstream files match both their recorded SHA-256 and Git blob SHA-1 in `SOURCE.json`. The immutable SDK revision is present in Cargo.toml/Cargo.lock.
- The fixture harness launches the real process over JSONL, implements a loopback HTTP GraphQL fixture, checks the synthetic Authorization header, captures host reverse RPCs, and exercises every declared command ID. The reported 51 requests and ten synthetic mutations are reported run results; I inspected the harness but did not rerun it or independently certify those counts.
- Its production smoke checks setup/error UI and protocol identity without a key and asserts zero HTTP requests. It does not establish successful real-account execution of all 14 commands. The report accurately distinguishes this from the fixture run.
- The two Rust unit tests cover dates and accepted web URL schemes. The fixture checks are materially broader than those unit tests, but do not cover the two state transitions above.

## Contracts inspected without a confirmed defect

Production transport fixes the endpoint to `https://api.linear.app/graphql`; the alternate endpoint exists only behind the `fixture` feature and is restricted to HTTP on `127.0.0.1`. Redirects are disabled. The package helper rejects binaries containing the fixture endpoint marker. Requests use a sensitive Authorization header, bounded response streaming (4 MiB), connect/overall deadlines and static GraphQL operations with bound variable objects. Arbitrary GraphQL error messages are replaced with bounded generic diagnostics, so the fixture's echoed synthetic key is not returned to the UI. No explicit mutation retry loop was found.

Pagination validates cursors, detects repeated cursors, deduplicates IDs and uses bounded collections. Lists stop after the implementation's 400-row or 30-cursor bound; actual large-workspace completeness and the stated tray cap remain limitations. The fixture represents the expected API shapes and is not schema/server validation of every query with a personal API key.

Form option IDs are checked against loaded choices, dates and text lengths are validated, and issue/project archival requires an explicit host confirmation. Actions are held in serialized session state, consumed before execution and regenerated with a new revision on rendering; the handler catches operation errors and commits the resulting notice/state, avoiding an automatic replay of the same callback. `RejectWhileBusy` and SDK cancellation/state fencing are used. New explicit user attempts after an uncertain network outcome can still retry a mutation; no server-side exactly-once claim is made.

The tray holds its state mutex across refresh/publication and events, uses a root-owned spawned task and a non-durable acknowledged background lease, and releases that lease on Stop. Its key is memory-only until stop/process exit, as documented. Successful native Mac execution, actual tray/GPUI behavior and real account permissions remain unverified by this review.

Managed Raycast OAuth, inbound AI tools/resource providers, several secondary form/document workflows and legacy cache migration are explicit implementation gaps. Having all 14 command entry points does not mean those omitted workflows or live-account parity have been accepted.
