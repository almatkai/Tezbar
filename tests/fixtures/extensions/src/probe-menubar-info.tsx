import React from 'react'
import { Icon, MenuBarExtra } from '@raycast/api'
export default function ProbeMenuBarInfo() {
  return <MenuBarExtra icon={Icon.Gear} tooltip="Probe info">
    <MenuBarExtra.Item title="CPU 23%" />
    <MenuBarExtra.Item title="Refresh" onAction={() => {}} />
  </MenuBarExtra>
}
