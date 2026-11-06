# M3A review repairs

Original review: [Opus at c741388](../reviews/GPUI_M3A_OPUS_c741388.md).
The controller slice is not accepted yet. This record separates implemented
repairs from fresh independent acceptance and from native-view integration.

The parent reproduced three small findings before repairing them. All three
regressions failed in `/tmp/tezbar-native-m03-parent-small-before.log`:
Settings opened AI instead of General, pending Enter fired at its exact expiry
depending on callback order, and a version-1 UI-state file missing a newer field
failed to decode. The repaired run passes all four targeted cases in
`/tmp/tezbar-native-m03-parent-small-after.log`.

| Finding | Implemented repair / status |
| --- | --- |
| M8 Settings route | `open-settings` selects General; `open-providers` selects AI. |
| M8 updater route | All three command IDs emit one `router::Effect::UpdateAction`. M5 must resolve this intent against current updater status, including manual download and busy no-op; this table does not claim that executor is integrated. |
| Low Enter boundary | Both expiry paths reject the pending Enter at `now >= expiry`. The regression covers either callback order. |
| Low native UI-state defaults | Missing fields use the version-1 defaults while explicit invalid types and unsupported versions remain errors; unknown fields survive decoding. |
| H1/H2 and remaining Medium/Low findings | Repair pending; all actionable findings require disposition and fresh Opus review. |

The immutable report is copied verbatim, SHA-256
`010f95a36b90bfe9ac576353f6ff427c94454e2ec56c3075b8f48f79f854a98c`.
Existing TS test sources are unchanged. No native surface or full migration
parity is inferred from these focused controller tests.
