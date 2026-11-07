import React from 'react'
import { Detail } from '@raycast/api'
const markdown = '# Native document\n\nҚазақша 日本語 😀\n\n```ts\nconst result = 42\n```\n\n![Fixture pixel](pixel.png)\n\n| Name | Value |\n| --- | --- |\n| SDK | Real |\n\n<strong>Allowed HTML</strong><br><em>Emphasis</em><script>not executable</script>\n\n[Documentation](https://example.invalid/docs)'
export default function FixtureDetail() {
  return <Detail navigationTitle="Fixture Detail" markdown={markdown} metadata={<Detail.Metadata>
    <Detail.Metadata.Label title="Runtime" text="Bun → core → GPUI" />
    <Detail.Metadata.Link title="Source" text="Source code" target="https://example.invalid/source" />
    <Detail.Metadata.TagList title="Tags"><Detail.Metadata.TagList.Item text="Native" /><Detail.Metadata.TagList.Item text="SDK" /></Detail.Metadata.TagList>
    <Detail.Metadata.Separator />
  </Detail.Metadata>} />
}
