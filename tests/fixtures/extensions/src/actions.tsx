import React, { useState } from 'react'
import { Action, ActionPanel, Clipboard, Detail, showHUD } from '@raycast/api'
function Child() { return <Detail navigationTitle="Child" markdown="Pushed child" actions={<ActionPanel><Action.Pop title="Pop Child" /></ActionPanel>} /> }
export default function FixtureActions() {
  const [hits, setHits] = useState(0)
  return <Detail navigationTitle="Fixture Actions" markdown={`Actions: ${hits}`} actions={<ActionPanel>
    <ActionPanel.Section title="Primary"><Action title="Count" shortcut={{ modifiers: ['cmd'], key: 'j' }} onAction={() => setHits(n => n + 1)} /><Action title="Destructive Count" style={Action.Style.Destructive} onAction={() => setHits(n => n - 1)} /></ActionPanel.Section>
    <ActionPanel.Section title="Desktop effects"><Action.CopyToClipboard title="Copy" content={'long copy:' + 'x'.repeat(10000)} /><Action.Paste title="Paste" content="Fixture paste Қаза🐎" /><Action.OpenInBrowser title="Open" url="https://example.invalid/open" /><Action.ShowInFinder title="Reveal" path="/fixture/reveal" /></ActionPanel.Section>
    <ActionPanel.Section title="Navigation"><Action.Push title="Push" target={<Child />} /><Action title="Effect Wave" onAction={async () => { await Clipboard.copy('long copy:' + 'x'.repeat(10000)); await showHUD('identical notice') }} /></ActionPanel.Section>
  </ActionPanel>} />
}
