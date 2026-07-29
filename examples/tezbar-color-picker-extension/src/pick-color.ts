/**
 * Tezbar Color Picker
 *
 * The extension intentionally contains no platform-specific implementation.
 * Tezbar supplies the native sampler through the Raycast-compatible `swift:`
 * bridge, so this repository can be released unchanged for macOS and Windows.
 */
export default async function PickColor(): Promise<null> {
  const { pickColor } = await import('swift:../swift')
  await pickColor()
  return null
}
