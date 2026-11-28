import React from 'react'
import { Action, ActionPanel, closeMainWindow, Detail, showHUD, showToast, Toast } from '@raycast/api'
export default function FixtureFeedback() {
  return <Detail navigationTitle="Fixture Feedback" markdown="Toast, HUD and close" actions={<ActionPanel>
    <Action title="Toast" onAction={async () => { const toast = await showToast({ style: Toast.Style.Animated, title: 'Working', message: 'Fixture work' }); toast.style = Toast.Style.Success; toast.title = 'Done'; await toast.show() }} />
    <Action title="HUD" onAction={async () => { await showHUD('Fixture HUD') }} />
    <Action title="HUD and Close" onAction={async () => { await showHUD('Fixture closed'); await closeMainWindow() }} />
  </ActionPanel>} />
}
