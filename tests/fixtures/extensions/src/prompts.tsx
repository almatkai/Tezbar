import React, { useState } from 'react'
import { Action, ActionPanel, Alert, confirmAlert, Detail } from '@raycast/api'
export default function FixturePrompts() {
  const [answer, setAnswer] = useState('not asked')
  return <Detail navigationTitle="Fixture Prompts" markdown={`Answer: ${answer}`} actions={<ActionPanel><Action title="Confirm" onAction={async () => setAnswer(String(await confirmAlert({ title: 'Fixture confirmation', message: 'TestHost supplies the answer', primaryAction: { title: 'Allow', style: Alert.ActionStyle.Destructive }, dismissAction: { title: 'Cancel' } })))} /></ActionPanel>} />
}
