# Parent-owned launcher integration contracts

These contracts extend the frozen M4/M5 checkpoint `af1ccdd` to close two explicit
M5 integration gaps. They do not accept the unfinished command bar or full app.
M4's clean normal binary remains preserved separately with source `af1ccdd`,
`dirty:false`, `bench_control:false`, SHA-256
`83bd72c0d7e8f09ff5319e945adad647b9375e8ddba2d09afa65d5c745993ff0`.
Its receipt and source-stable integrated test logs are under
`/tmp/tezbar-native-m04-parent-repair-evidence/`.

## Saved pins

`launcher::controller::Input::ActivatePin { row: Row, rank: usize }` represents
an explicit gesture over the canonical saved pins in `UiState`. It is separate
from a generation-bearing result-list click: the current search need not
contain the saved row. Adapters must not use this input for speculative search
previews. The controller refuses hidden/composing/busy activation and temporary
or non-pinnable rows. It uses the normal ticket/completion flow, preserving note
identity and leaving the search list unchanged. Keyboard adapters still reject
held shortcut repeats before dispatch.

Regressions exercise a pin absent from authoritative search, duplicate dispatch,
failed activation and retry, stale completion, hide, IME and saved note identity:
`saved_pin_has_its_own_activation_authority_and_uses_normal_tickets` and
`saved_pin_respects_composition_and_keeps_typed_note_identity`.

## Answer-stream ownership

The existing `query` channel accepts an **optional second positional argument**:
a non-empty opaque owner string of at most 256 UTF-8 bytes. Invalid owners are
rejected before starting work. Native requests send `[text, owner]`; an owned
answer emits `{ "owner": owner, "value": originalPayload }` on `stream-token`,
`stream-error` and `stream-done`. The original token/error strings and completion
null remain unchanged inside `value`. Callers omitting the owner retain the exact
legacy String/Null event shapes and unchanged intent result.

The existing internal run-identity check still rejects superseded producers.
Native views additionally match their own unique owner before applying events,
so already-queued output cannot contaminate the next answer. Owners must be
unique across view recreation; a process-wide sequence is sufficient. Keep the
subscription installed before starting the query and drain/ignore retired
events rather than attributing unowned legacy payloads to the current request.

The loopback regression failed on the original payload shape in
`/tmp/tezbar-native-m05-parent-query-owner-before.log`. Three production-path
regressions cover owned token/completion, unchanged legacy payloads, provider
error/completion and invalid-owner rejection before work. All **152 AI tests**
passed with two test threads in `/tmp/tezbar-native-m05-parent-ai-tests.log`;
no model service, credentials or real profile was used. The complete **363-case
model suite** passed on Rust 1.98.1 and 1.85 in
`/tmp/tezbar-native-m05-parent-model-tests.log` and
`/tmp/tezbar-native-m05-parent-model-msrv.log`. The M5 view owner must still
verify its actual slot shortcuts, event filtering and picker interaction.

Fresh Opus review, native visual/platform acceptance and full-app RAM measurement
remain pending. Windows validation currently targets the older immutable M3
source `42e22e4`, and cannot establish acceptance of these later interfaces.
