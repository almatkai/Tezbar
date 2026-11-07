import React, { useState } from 'react'
import { Action, ActionPanel, Icon, List } from '@raycast/api'
export default function FixtureList() {
  const [server, setServer] = useState(false)
  const [query, setQuery] = useState('')
  const [count, setCount] = useState(30)
  const [category, setCategory] = useState('all')
  const [loading, setLoading] = useState(false)
  const items = Array.from({ length: count }, (_, i) => ({ id: `item-${i}`, title: `Item ${i}`, subtitle: i % 2 ? 'Odd' : 'Even' }))
  return <List navigationTitle="Fixture List" searchBarPlaceholder="Search fixture items" filtering={!server} isLoading={loading} isShowingDetail
    onSearchTextChange={server ? setQuery : undefined}
    searchBarAccessory={<List.Dropdown value={category} onChange={setCategory}><List.Dropdown.Section title="Categories"><List.Dropdown.Item title="All" value="all" /><List.Dropdown.Item title="Even" value="even" /></List.Dropdown.Section></List.Dropdown>}
    pagination={{ hasMore: count < 1000, pageSize: 30, onLoadMore: () => setCount(n => Math.min(1000, n + 30)) }}>
    <List.EmptyView title="No fixture items" description="Change the query" icon={Icon.MagnifyingGlass} />
    <List.Section title="Items" subtitle={`${count} · ${category}`}>
      {items.filter(item => (!server || item.title.includes(query)) && (category !== 'even' || item.subtitle === 'Even')).map(item => <List.Item key={item.id} id={item.id} title={item.title} subtitle={item.subtitle} icon={Icon.Star} keywords={['fixture', 'Қазақша']}
        accessories={[{ text: 'Accessory' }, { tag: 'Tag' }, { date: new Date('2026-11-07T00:00:00Z') }]}
        detail={<List.Item.Detail markdown={`# ${item.title}\n\nActual SDK detail`} metadata={<List.Item.Detail.Metadata><List.Item.Detail.Metadata.Label title="Identifier" text={item.id} /><List.Item.Detail.Metadata.Link title="Docs" text="Fixture docs" target="https://example.invalid/docs" /><List.Item.Detail.Metadata.TagList title="Tags"><List.Item.Detail.Metadata.TagList.Item text="Native" /></List.Item.Detail.Metadata.TagList><List.Item.Detail.Metadata.Separator /></List.Item.Detail.Metadata>} />}
        actions={<ActionPanel><Action title="Toggle Server Search" onAction={() => setServer(v => !v)} /><Action title="Toggle Loading" onAction={() => setLoading(v => !v)} /><Action title="Grow to 1000" onAction={() => setCount(1000)} /></ActionPanel>} />)}
    </List.Section>
  </List>
}
