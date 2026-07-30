import type { ExtensionManifest } from './extensions'

/**
 * Tezbar-maintained extensions that are safe to recommend outside the Raycast
 * store. Keep these as repository URLs so they can be installed directly from
 * GitHub on Windows.
 */
export const LOVED_EXTENSIONS: ExtensionManifest[] = [
  {
    id: 'raycast.tezbar-color-picker',
    name: 'Color Picker',
    description:
      'Pick an exact pixel from any screen and inspect it in RGB, HEX, HSL, HSV, and CSS formats.',
    author: 'Tezbar',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/tezbar-color-picker-extension',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/tezbar-color-picker-extension/master/assets/icon.svg',
    screenshotUrls: [
      'https://raw.githubusercontent.com/almatkai/tezbar-color-picker-extension/master/assets/picker-preview.png',
      'https://raw.githubusercontent.com/almatkai/tezbar-color-picker-extension/master/assets/color-wheel-preview.png',
    ],
    categories: ['Design', 'Developer Tools'],
    commands: [
      {
        name: 'pick-color',
        title: 'Pick Color',
        description: 'Open the native screen sampler and inspect the selected color.',
      },
    ],
  },
]

function searchableText(extension: ExtensionManifest): string {
  return [
    extension.name,
    extension.id,
    extension.description,
    extension.author,
    extension.owner,
    extension.repository,
    ...(extension.categories ?? []),
    ...(extension.commands ?? []).flatMap((command) => [
      command.name,
      command.title,
      command.description,
    ]),
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase()
}

/** Search the curated list without contacting the Raycast extension store. */
export function searchLovedExtensions(query: string): ExtensionManifest[] {
  const normalizedQuery = String(query || '')
    .trim()
    .toLowerCase()
  if (!normalizedQuery) return [...LOVED_EXTENSIONS]

  return LOVED_EXTENSIONS.filter((extension) => searchableText(extension).includes(normalizedQuery))
}
