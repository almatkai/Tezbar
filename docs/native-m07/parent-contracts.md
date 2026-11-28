# Parent-owned M7 shared contracts

These additions support the real Settings/onboarding adapter. They do not close
M7, native view parity, independent review, Windows, or physical acceptance.

## Extension contributions and saved secrets

`Contributions::update` accepts canonical `agents`, not `profiles`. Updates and
`edit_row` emit only the changed field or collection. The core applies that patch
to its current record and validates the complete manifest. Untouched fields and
opaque refs remain in the canonical store instead of being copied from a stale
view. Provider definitions now use the same validation path; sensitive literal
headers are encrypted into the existing provider namespace, and changing provider
configuration disables the extension before further use.

`secret_slots(&self, extension) -> Vec<SecretDescriptor>` returns display-only
`label` and `scope` plus an opaque `slot`. `SecretSlot` has private fields, Clone,
and no Debug/serde. `set_saved_secret(&mut self, &SecretSlot, value)` validates the
loaded revision and the exact declared connection/provider namespace before
emitting a SetSecret effect. The view never recovers a ref ID from JSON or a label.
Reopening a freshly loaded manifest invalidates old slots.

## Task overrides

`Edit::TaskSelection { task, provider, model }` merges its task into fresh provider
and model override maps. Empty values remove the corresponding override. A task
has its own intent key, so an old failed retry cannot replace a newer successful
choice; other tasks survive. Provider removal repairs every task using that
provider. A global activation respects a newer explicit chat override.

## Voice download cancellation

`voice:model:cancel-download`, with `[{ "modelId": id }]`, returns
`{ "ok": true, "cancelled": bool }`. It cancels and awaits only that model's
owned provisioning job. The partial file is removed before completion is reported;
finished assets, other model jobs and speech are preserved. Repeated starts share
the active owner. Cancelling an already finished model returns false. Shutdown
cancels all owned download tokens in addition to its existing stop/speech path.
No Cancel action is implemented by deleting the model or stopping all voice work.

## Validation

- Combined model suite: **373 passed** on Rust 1.98.1 and 1.85, zero ignored.
- Actual core agent/provider update: one passed, including invalid-URL rejection,
  preserved old refs, encrypted new header and durable reread.
- Entire voice namespace: **5 passed**, including a loopback HTTP cancellation
  that preserves a concurrent download, finished asset and speech owner.
- Existing provider-secret namespace regression: one passed.

Logs: `/tmp/tezbar-m7-parent-models-1.log`,
`/tmp/tezbar-m7-parent-models-msrv-1.log`, and
`/tmp/tezbar-native-m7-parent-seams/{core-extension-2,core-voice,core-provider-secrets}.log`.
The secrets tests use the core's test-only MemoryKey and synthetic values; they
make no OS credential-store call. No real model download, installation, provider
request, shipping profile, lockfile/dependency change, release or Actions run
occurred. Native M7 integration and its final frozen gates remain separate.
