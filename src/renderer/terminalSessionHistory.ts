/**
 * Native output can arrive while a durable history snapshot is being loaded.
 * Return only the buffered suffix that is not already present in the snapshot.
 */
export function terminalOutputAfterSnapshot(snapshot: string, buffered: string): string {
  const maxOverlap = Math.min(snapshot.length, buffered.length)
  for (let overlap = maxOverlap; overlap > 0; overlap -= 1) {
    if (snapshot.endsWith(buffered.slice(0, overlap))) {
      return buffered.slice(overlap)
    }
  }
  return buffered
}
