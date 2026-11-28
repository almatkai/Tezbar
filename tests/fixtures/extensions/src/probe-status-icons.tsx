import React, { useState } from 'react'
import { Icon, MenuBarExtra } from '@raycast/api'
export default function StatusIcons(props: { arguments?: { icon?: string } }) {
  const [icon, setIcon] = useState<unknown>(props.arguments?.icon || Icon.Checkmark)
  const [hidden, setHidden] = useState(false)
  if (hidden) return null
  return <MenuBarExtra icon={icon as never} tooltip="Declared icon fixture">
    <MenuBarExtra.Item title="Gear" onAction={() => setIcon(Icon.Gear)} />
    <MenuBarExtra.Item title="Checkmark" onAction={() => setIcon(Icon.Checkmark)} />
    <MenuBarExtra.Item title="Gear then SVG" onAction={() => { setIcon(Icon.Gear); setTimeout(() => setIcon('assets/status.svg'), 150) }} />
    <MenuBarExtra.Item title="Local PNG" onAction={() => setIcon('assets/pixel.png')} />
    <MenuBarExtra.Item title="Local SVG" onAction={() => setIcon('assets/status.svg')} />
    <MenuBarExtra.Item title="Unsupported" onAction={() => setIcon('raycast-NotARealIcon')} />
    <MenuBarExtra.Item title="Remove" onAction={() => setHidden(true)} />
  </MenuBarExtra>
}
