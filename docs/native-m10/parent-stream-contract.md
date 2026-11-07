> Current linked status and superseding evidence: [linked-handoff.md](linked-handoff.md). The original draft below is retained as historical context.

# Parent M10 stream/input requirements

This is a required integration design, not an implemented API or passing test.
The source-only terminal currently uses legacy Attach/String writes. Those routes
must remain compatible until the native cutover. New first-party requests use
`core:terminal:*`; do not add them to generic extension/renderer channel lists.

## Ordered bounded replay

Do not use `read_persisted_history`'s whole-file String or tail raw VT bytes.
A bounded slice can begin inside UTF-8, CSI/OSC, alternate-screen or SGR state.
Parse from a known stream start/checkpoint in bounded chunks, preserving the one
Alacritty Processor, and yield between parse turns. Retained grid/history remains
bounded at 100,000 lines. Replay must not execute historical terminal device replies
or OSC52 clipboard actions.

Add a finite attach/replay interface around the EXISTING managed PTY owner:

- Stable session ID plus a distinct PTY/stream generation for each actual process.
- Snapshot watermark and opaque replay handle; no user-provided file path. Opening
  a snapshot and publishing output offsets use one per-stream ordering boundary.
- Bounded reads (at most 64 KiB raw bytes), explicit next offset/end watermark and
  exact bytes (base64 on the finite Value wire). No unbounded response allocation.
- Cursor/FD registry is bounded, explicitly releasable and idle-expiring. A dropped
  UI owner releases it; no file handle may prevent clear/delete indefinitely.
- Live data/exit include generation and output position. Old-generation events and
  overlapped snapshot bytes are rejected by identity/position, not text matching.
  Repeated identical output is legitimate and must never be deduplicated by text.
- If live output outruns the bounded UI queue, recover from the durable byte cursor;
  do not drop an arbitrary segment, silently reset VT state or grow an unbounded map.
- Disk errors/truncation/deletion must give a truthful unavailable/restart result.
  Never claim a durable byte offset that was not actually persisted. Keep direct
  live handling separate from recoverability if disk persistence fails.

The core reader must not publish while holding the global sessions lock, and replay
must not wait on UI progress under a lock required by exit/kill/shutdown. Final data
is ordered before exit. All old Tauri create/attach/notifications stay compatible.

## Atomic binary input admission

Add a first-party generation-fenced byte-input request to the SAME TerminalInput
writer. Bounded base64 wire avoids a million JSON Number allocations. Validate
encoded/decoded size, generation/session and the complete input before enqueue.
An entire user paste, including bracketed-paste delimiters, is admitted atomically
within the existing 1 MiB input queue budget, or rejected without sending a prefix.
Legacy X10 mouse bytes must be preserved, including non-UTF-8 octets. Keep legacy
String writes unchanged. A successful admission is not a promise that a process
survives to consume every byte. Never automatically replay after uncertain I/O.

## Verification obligation

Use actual production stream/input helpers with synthetic readers/writers and
private files: split UTF-8/CSI/OSC, repeated identical chunks, snapshot/live race,
restore with same ID/new generation, large history bounded reads, slow replay
recovery, cursor release/expiry, deletion/truncation/persistence failure, invalid
encoding, whole-paste capacity refusal and raw mouse bytes. A later explicitly
owned shell/PTY fixture can verify real process integration; fake transport/source
syntax alone cannot close these rows.

M8 currently owns Cargo/shared compiled integration. Do not edit core terminal,
exports or manifests until its safe-checkpoint release. Parent has requested that
release; this document grants no permission to race its frozen build inputs.
