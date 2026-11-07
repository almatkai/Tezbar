import React, { useState } from 'react'
import { MenuBarExtra } from '@raycast/api'
export default function FixtureMenuBar() {
  const [count, setCount] = useState(0)
  const [removed, setRemoved] = useState(false)
  if (removed) return null
  return <MenuBarExtra title={`Fixture ${count}`} tooltip="Fixture status item">
    <MenuBarExtra.Section title="Status"><MenuBarExtra.Item title={`Count ${count}`} onAction={() => setCount(n => n + 1)} /></MenuBarExtra.Section>
    <MenuBarExtra.Submenu title="More"><MenuBarExtra.Item title="Increment" onAction={() => setCount(n => n + 1)} /></MenuBarExtra.Submenu>
    <MenuBarExtra.Item title="Later" onAction={() => { setTimeout(() => setCount(n => n + 1), 150) }} />
    <MenuBarExtra.Item title="Hide briefly" onAction={() => { setRemoved(true); setTimeout(() => setRemoved(false), 500) }} />
    <MenuBarExtra.Separator />
    <MenuBarExtra.Item title="Remove" onAction={() => setRemoved(true)} />
  </MenuBarExtra>
}
