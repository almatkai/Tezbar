import React, { useEffect, useState } from 'react'
import { Action, ActionPanel, Detail } from '@raycast/api'
export default function FixtureInvalidation() {
  const [revision, setRevision] = useState(0)
  useEffect(() => { const timer = setTimeout(() => setRevision(n => n + 1), 100); return () => clearTimeout(timer) }, [])
  return <Detail navigationTitle="Fixture Invalidation" markdown={`Revision ${revision}`} actions={<ActionPanel><Action title="Invalidate Twice" onAction={() => { setRevision(n => n + 1); setRevision(n => n + 1) }} /></ActionPanel>} />
}
