import React, { useState } from 'react'
import { Action, ActionPanel, Grid } from '@raycast/api'
export default function FixtureGrid() {
  const [empty, setEmpty] = useState(false)
  return <Grid navigationTitle="Fixture Grid" columns={3} filtering isLoading={false}>
    <Grid.EmptyView title="Empty fixture grid" description="No artwork" />
    <Grid.Section title="Colours" subtitle="Three columns">
      {!empty && ['#ba3e33', '#447954', '#cfaa52', '#44667f', '#aa7766', '#8a8f83'].map((color, i) => <Grid.Item key={color} id={`colour-${i}`} title={`Colour ${i}`} subtitle={color} content={{ color }} actions={<ActionPanel><Action title="Empty Grid" onAction={() => setEmpty(true)} /></ActionPanel>} />)}
    </Grid.Section>
    <Grid.Section title="Artwork">{!empty && <Grid.Item id="artwork" title="Fixture image" content={{ source: 'pixel.png' }} />}</Grid.Section>
  </Grid>
}
