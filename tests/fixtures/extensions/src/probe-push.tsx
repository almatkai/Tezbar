import React from 'react'
import { Action, ActionPanel, Detail, List, useNavigation } from '@raycast/api'
function Child() { return <Detail navigationTitle="Slow Child" markdown="Pushed slow child" /> }
export default function ProbePush() {
  const { push } = useNavigation()
  return <List navigationTitle="Probe Push" searchBarPlaceholder="Search probe">
    {['Alpha', 'Beta', 'Gamma'].map(t => <List.Item key={t} id={t} title={t} actions={<ActionPanel><Action title="Slow Push" onAction={async () => { await new Promise(r => setTimeout(r, 600)); push(<Child />) }} /></ActionPanel>} />)}
  </List>
}
