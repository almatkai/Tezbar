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
  {
    id: 'raycast.asciimath-to-latex-converter',
    name: 'AsciiMath to LaTeX Converter',
    description: 'Convert AsciiMath expressions to LaTeX with live preview.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/asciimath-to-latex-converter',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/asciimath-to-latex-converter/main/assets/icon.png',
    categories: ['Productivity'],
    commands: [
      {
        name: 'convert-asciimath-to-latex',
        title: 'Convert AsciiMath to LaTeX',
        description: 'Convert AsciiMath input to LaTeX and preview the result.',
      },
    ],
  },
  {
    id: 'raycast.base64',
    name: 'Base64',
    description: 'Encode and decode Base64 text from Tezbar.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/base64',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/base64/main/assets/icon.svg',
    categories: ['Developer Tools', 'Data'],
    commands: [
      {
        name: 'index',
        title: 'Base64',
        description: 'Encode or decode Base64 text interactively.',
      },
      {
        name: 'encode',
        title: 'Base64 Encode Clipboard',
        description: 'Replace clipboard text with its Base64 representation.',
      },
      {
        name: 'decode',
        title: 'Base64 Decode Clipboard',
        description: 'Decode Base64 clipboard text and copy the result.',
      },
    ],
  },
  {
    id: 'raycast.base64-to-file',
    name: 'Base64 to File',
    description: 'convert base64 to file',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/base64-to-file',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/base64-to-file/main/assets/command-icon.png',
    categories: ['Data', 'Applications'],
    commands: [
      {
        name: 'index',
        title: 'Convert Base64 to File',
        description: 'Convert base64 text to file',
      },
    ],
  },
  {
    id: 'raycast.bitly-url-shortener',
    name: 'Bitly URL Shortener',
    description: 'Quickly shorten the selected URL or current clipboard URL with Bitly',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/bitly-url-shortener',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/bitly-url-shortener/main/assets/command-icon.png',
    categories: ['Communication'],
    commands: [
      {
        name: 'index',
        title: 'Shorten URL',
        description: 'Overwrite the clipboard contents with a shortened URL',
      },
      {
        name: 'list-links',
        title: 'List Links',
        description: 'List all the links you have shortened with Bitly',
      },
    ],
  },
  {
    id: 'raycast.blurhash',
    name: 'BlurHash',
    description: 'Generate blurhash from clipboard content or Finder',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/blurhash',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/blurhash/main/assets/extension-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'clipboard-blurhash',
        title: 'BlurHash',
        description: 'Generate blurhash from clipboard content or Finder',
      },
    ],
  },
  {
    id: 'raycast.change-case',
    name: 'Change Case',
    description: 'Transform a string between camelCase, snake_case, CONSTANT_CASE, and more',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/change-case',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/change-case/main/assets/command-icon.png',
    categories: ['Developer Tools', 'Productivity'],
    commands: [
      {
        name: 'change-case',
        title: 'Change Case',
        description: 'Change the case of the selected text or text in the clipboard',
      },
      {
        name: 'convert-camel-case',
        title: 'Convert to camelCase',
        description: 'Convert selected text or clipboard to camelCase',
      },
      {
        name: 'convert-capital-case',
        title: 'Convert to Capital Case',
        description: 'Convert selected text or clipboard to Capital Case',
      },
      {
        name: 'convert-constant-case',
        title: 'Convert to CONSTANT_CASE',
        description: 'Convert selected text or clipboard to CONSTANT_CASE',
      },
      {
        name: 'convert-dot-case',
        title: 'Convert to dot.case',
        description: 'Convert selected text or clipboard to dot.case',
      },
      {
        name: 'convert-header-case',
        title: 'Convert to Header-Case',
        description: 'Convert selected text or clipboard to Header-Case',
      },
      {
        name: 'convert-lower-case',
        title: 'Convert to lower case',
        description: 'Convert selected text or clipboard to lower case',
      },
      {
        name: 'convert-lower-first',
        title: 'Convert to lower First',
        description: 'Convert selected text or clipboard to lower First',
      },
      {
        name: 'convert-no-case',
        title: 'Convert to no case',
        description: 'Convert selected text or clipboard to no case',
      },
      {
        name: 'convert-kebab-case',
        title: 'Convert to kebab-case',
        description: 'Convert selected text or clipboard to kebab-case',
      },
      {
        name: 'convert-kebab-upper-case',
        title: 'Convert to KEBAB-UPPER-CASE',
        description: 'Convert selected text or clipboard to KEBAB-UPPER-CASE',
      },
      {
        name: 'convert-pascal-case',
        title: 'Convert to PascalCase',
        description: 'Convert selected text or clipboard to PascalCase',
      },
      {
        name: 'convert-pascal-snake-case',
        title: 'Convert to Pascal_Snake_Case',
        description: 'Convert selected text or clipboard to Pascal_Snake_Case',
      },
      {
        name: 'convert-path-case',
        title: 'Convert to path/case',
        description: 'Convert selected text or clipboard to path/case',
      },
      {
        name: 'convert-random-case',
        title: 'Convert to rAndOm cAsE',
        description: 'Convert selected text or clipboard to rAndOm cAsE',
      },
      {
        name: 'convert-sentence-case',
        title: 'Convert to Sentence case',
        description: 'Convert selected text or clipboard to Sentence case',
      },
      {
        name: 'convert-snake-case',
        title: 'Convert to snake_case',
        description: 'Convert selected text or clipboard to snake_case',
      },
      {
        name: 'convert-alternating-case',
        title: 'Convert to aLtErNaTiNg cAsE',
        description: 'Convert selected text or clipboard to aLtErNaTiNg cAsE',
      },
      {
        name: 'convert-swap-case',
        title: 'Convert to sWAP cASE',
        description: 'Convert selected text or clipboard to sWAP cASE',
      },
      {
        name: 'convert-title-case',
        title: 'Convert to Title Case',
        description: 'Convert selected text or clipboard to Title Case',
      },
      {
        name: 'convert-upper-case',
        title: 'Convert to UPPER CASE',
        description: 'Convert selected text or clipboard to UPPER CASE',
      },
      {
        name: 'convert-upper-first',
        title: 'Convert to Upper first',
        description: 'Convert selected text or clipboard to Upper first',
      },
    ],
  },
  {
    id: 'raycast.clipboard-editor',
    name: 'Clipboard Editor',
    description: 'Edit your clipboard text',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/clipboard-editor',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/clipboard-editor/main/assets/icon.png',
    categories: ['Productivity'],
    commands: [
      {
        name: 'edit-clipboard-text',
        title: 'Edit Clipboard Content',
        description: 'Edit your clipboard text',
      },
    ],
  },
  {
    id: 'raycast.clipboard-formatter',
    name: 'Clipboard Formatter',
    description:
      'Removes formatting and html from the text stored in the clipboard, returning the unformatted text to the clipboard.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/clipboard-formatter',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/clipboard-formatter/main/assets/extension_icon.png',
    categories: ['Productivity', 'System'],
    commands: [
      {
        name: 'index',
        title: 'Strip Clipboard Formatting',
        description:
          'Removes formatting and html from the text stored in the clipboard, returning the unformatted text to the clipboard.',
      },
    ],
  },
  {
    id: 'raycast.clipboard-sequential-paste',
    name: 'Clipboard Sequential Paste',
    description: 'Copy a list to your clipboard and paste items one by one.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/clipboard-sequential-paste',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/clipboard-sequential-paste/main/assets/extension-icon.png',
    categories: ['Productivity'],
    commands: [
      {
        name: 'paste-next-item',
        title: 'Paste Next Item',
        description: 'Pastes the next item from your copied list.',
      },
    ],
  },
  {
    id: 'raycast.code-review-emojis',
    name: 'Code Review Emoji Guide',
    description:
      'A simple emoji legend to help convey intention and added meaning in code review comments.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/code-review-emojis',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/code-review-emojis/main/assets/command-icon.png',
    categories: ['Documentation', 'Developer Tools'],
    commands: [
      {
        name: 'index',
        title: 'Show Review Emoji',
        description:
          'Shows a list of emojis that can be used for code review, in line with the Code Review Emoji Guide.',
      },
    ],
  },
  {
    id: 'raycast.color-shades',
    name: 'Color Shades',
    description:
      'Generate color shades from a base color and manage your generated color shades palettes.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/color-shades',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/color-shades/main/assets/command-icon.png',
    categories: ['Design Tools', 'Developer Tools'],
    commands: [
      {
        name: 'generate-color-shades',
        title: 'Generate Color Shades',
        description: 'Generate color shades palette for the given base color.',
      },
      {
        name: 'view-color-shades',
        title: 'View Color Shades',
        description: 'View your generated color shades palettes.',
      },
    ],
  },
  {
    id: 'raycast.convert',
    name: 'Web Converter',
    description:
      'Converts everything web related: rem, px, pt, hex, hex (with opacity), rgb, rgba, hsl, hsla, oklch and shows the closest Tailwind CSS color.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/convert',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/convert/main/assets/icon.png',
    categories: ['Developer Tools', 'Productivity', 'Web'],
    commands: [
      {
        name: 'convert',
        title: 'Convert Unit',
        description:
          'Converts everything web related: rem, px, pt, hex, hex (with opacity), rgb, rgba, hsl, hsla, oklch and shows the closest Tailwind CSS color.',
      },
    ],
  },
  {
    id: 'raycast.convert-3d-models',
    name: 'Convert 3D Models',
    description: 'An extension to convert 3D models to different formats.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/convert-3d-models',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/convert-3d-models/main/assets/extension_icon.png',
    categories: ['Design Tools', 'Productivity', 'Other'],
    commands: [
      {
        name: 'convert',
        title: 'Convert 3D Model Files',
        description: 'Convert the selected model to the specified format.',
      },
      {
        name: 'quickConvertSTL',
        title: 'Quick Convert to STL',
        description: 'Quickly convert selected models to STL',
      },
      {
        name: 'quickConvertOBJ',
        title: 'Quick Convert to OBJ',
        description: 'Quickly convert selected models to OBJ',
      },
      {
        name: 'quickConvertSTP',
        title: 'Quick Convert to STP',
        description: 'Quickly convert selected models to STP',
      },
    ],
  },
  {
    id: 'raycast.converter',
    name: 'Converter',
    description:
      'Arbitrary input, direct output. Includes Base converter, Byte converter, Code converter, etc.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/converter',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/converter/main/assets/extension-icon.png',
    categories: ['Design Tools', 'Developer Tools', 'Productivity'],
    commands: [
      {
        name: 'base-converter',
        title: 'Base Converter',
        description:
          'Convert number from any base to any base: decimal, binary, octal, hexadecimal, etc.',
      },
      {
        name: 'code-converter',
        title: 'Code Converter',
        description:
          'Convert between different character encodings: Unicode, Base64, UTF-8, ASCII, &#xXXXX, URL.',
      },
      {
        name: 'byte-converter',
        title: 'Byte Converter',
        description: 'Convert among different byte unit: b, B, KB, MB, GB, etc.',
      },
    ],
  },
  {
    id: 'raycast.convert-px-to-vw-vh',
    name: 'Pixels to Viewport Width or Height',
    description: 'Fast conversion of pixels to vw or vh depending on the size of your viewport.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/convert-px-to-vw-vh',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/convert-px-to-vw-vh/main/assets/logo.png',
    categories: ['Developer Tools', 'Design Tools', 'Web', 'Productivity'],
    commands: [
      {
        name: 'index',
        title: 'Convert PX to VW or VH',
        description: 'All commands in one place for easy convertion',
      },
      { name: 'height', title: 'Pixels Viewport Height', description: 'Pixels to viewport height' },
      { name: 'width', title: 'Pixels Viewport Width', description: 'Pixels to viewport width' },
      {
        name: 'menu-bar',
        title: 'Conversion History',
        description: 'History of your last conversions',
      },
    ],
  },
  {
    id: 'raycast.convert-typescript-to-javascript',
    name: 'Convert TypeScript to JavaScript',
    description:
      'Remove type information from TypeScript code to make it compatible with JavaScript REPLs and other applications',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/convert-typescript-to-javascript',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/convert-typescript-to-javascript/main/assets/icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'paste-from-clipboard',
        title: 'Convert & Paste as JavaScript',
        description: 'Retrieve clipboard content, transform it to JavaScript, and paste it',
      },
      {
        name: 'copy-to-clipboard',
        title: 'Copy TypeScript as JavaScript',
        description:
          'Get selected text from the active application, transform it to JavaScript, and keep it in the clipboard',
      },
    ],
  },
  {
    id: 'raycast.counter',
    name: 'Counter',
    description: 'A simple counter, in the comfort of Raycast root!',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/counter',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/counter/main/command-icon.png',
    categories: ['Productivity', 'Fun'],
    commands: [
      {
        name: 'increment-counter',
        title: 'Increment Counter',
        description: 'Increments the count of the counter.',
      },
      {
        name: 'decrement-counter',
        title: 'Decrement Counter',
        description: 'Decrements the count of the counter.',
      },
      { name: 'reset-counter', title: 'Reset Counter', description: 'Resets the counter.' },
    ],
  },
  {
    id: 'raycast.count-numbers',
    name: 'Count Numbers',
    description: 'A simple but powerful number counter!',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/count-numbers',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/count-numbers/main/assets/extension_icon.png',
    categories: ['Fun', 'Productivity'],
    commands: [{ name: 'count-numbers', title: 'Count Numbers', description: 'Count numbers.' }],
  },
  {
    id: 'raycast.country-lookup',
    name: 'Country Lookup',
    description:
      'Search and explore detailed data for every country: flags, capitals, languages, currencies, regions and more, powered by the REST Countries API.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/country-lookup',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/country-lookup/main/assets/extension_iconv1.png',
    categories: ['Data', 'Web'],
    commands: [
      {
        name: 'index',
        title: 'Search Countries',
        description:
          'Search countries by name, capital, or language and browse rich details like flags, currencies, and regions.',
      },
    ],
  },
  {
    id: 'raycast.crypto-portfolio-tracker',
    name: 'Crypto Portfolio Tracker',
    description: 'Add crypto wallets by their addresses and track your entire portfolio.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/crypto-portfolio-tracker',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/crypto-portfolio-tracker/main/assets/extension-icon.png',
    categories: ['Data', 'Finance'],
    commands: [
      {
        name: 'add-ethereum-address',
        title: 'Add Ethereum Address',
        description: 'Import an Ethereum address to add to your portfolio.',
      },
      {
        name: 'portfolio',
        title: 'View Portfolio',
        description: 'View the total value and breakdown of your crypto portfolio.',
      },
    ],
  },
  {
    id: 'raycast.crypto-price',
    name: 'Crypto Price',
    description:
      'Keep an eye on Bitcoin (BTC), Ethereum (ETH), and other cryptocurrency prices and more directly from your system menu bar',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/crypto-price',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/crypto-price/main/assets/command-icon.png',
    categories: ['Finance'],
    commands: [
      { name: 'menu-bar', title: 'Crypto Price', description: 'See cryptocurency prices and more' },
    ],
  },
  {
    id: 'raycast.css-calculations',
    name: 'CSS Calculations',
    description: 'Useful CSS Calculations in Web Development',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/css-calculations',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/css-calculations/main/assets/css.png',
    categories: ['Developer Tools', 'Productivity'],
    commands: [
      {
        name: 'clamp_px',
        title: 'Calculate Clamp PX',
        description: 'Generate responsive clamp function based on px values',
      },
      {
        name: 'clamp_rem',
        title: 'Calculate Clamp REM',
        description: 'Generate responsive clamp function based on rem values',
      },
    ],
  },
  {
    id: 'raycast.csv-to-excel',
    name: 'Convert CSV to Excel',
    description: 'Convert CSV table to Excel compatible table.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/csv-to-excel',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/csv-to-excel/main/assets/command-icon.png',
    categories: ['Applications', 'Productivity'],
    commands: [
      {
        name: 'index',
        title: 'Convert CSV to Excel',
        description: 'Convert CSV table to Excel compatible table',
      },
    ],
  },
  {
    id: 'raycast.curl',
    name: 'cURL',
    description: 'Keyboard-first HTTP client',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/curl',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/curl/main/assets/app-icon.png',
    categories: ['Productivity', 'Developer Tools'],
    commands: [
      {
        name: 'index',
        title: 'Make HTTP Request',
        description: 'Shortcut to make an HTTP request',
      },
      { name: 'requests', title: 'Requests History', description: 'HTTP requests history' },
    ],
  },
  {
    id: 'raycast.currency-exchange',
    name: 'Currency Exchange',
    description:
      'Simple Currency Exchange with a selectable rate provider (ExchangeRate-API or UniRateAPI)',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/currency-exchange',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/currency-exchange/main/assets/currency.png',
    categories: ['Developer Tools'],
    commands: [
      { name: 'index', title: 'Currency Exchange', description: 'Calculate with Exchange Rate' },
    ],
  },
  {
    id: 'raycast.custom-wordle',
    name: 'Custom Wordle',
    description: 'Get a link to play wordle with your own word',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/custom-wordle',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/custom-wordle/main/assets/custom-wordle-icon.png',
    categories: ['Fun'],
    commands: [
      {
        name: 'create-custom-wordle',
        title: 'Create Custom Wordle',
        description:
          'Create a wordle with your own word and share it with your friends to have them guess your word.',
      },
    ],
  },
  {
    id: 'raycast.date-converter',
    name: 'Date Converter',
    description: 'Convert a variety of date formats',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/date-converter',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/date-converter/main/extension-icon.png',
    categories: ['Developer Tools', 'Productivity'],
    commands: [{ name: 'index', title: 'Convert Date', description: 'Parse and convert a date' }],
  },
  {
    id: 'raycast.datetime-format-converter',
    name: 'Date Format Converter',
    description: 'Convert timestamps and datetime strings into various formats.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/datetime-format-converter',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/datetime-format-converter/main/assets/command-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'index',
        title: 'Format Date',
        description: 'Convert timestamps and datetime strings into various formats.',
      },
      {
        name: 'insert-natural-language-date',
        title: 'Insert Natural Language Date',
        description:
          'Convert a natural language prompt into a formatted date and insert it into the frontmost application',
      },
    ],
  },
  {
    id: 'raycast.dictionary',
    name: 'Web Dictionaries',
    description:
      'Search any word with multiple online dictionaries or translation engines in one placeвЂ”lightweight, zero dependencies.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/dictionary',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/dictionary/main/assets/icon.png',
    categories: ['Productivity', 'Data', 'Web'],
    commands: [
      {
        name: 'fromCmd',
        title: 'Search in Dictionaries',
        description: 'Search any word with multiple online dictionaries.',
      },
      {
        name: 'fromSelected',
        title: 'Search From Cursor Selection',
        description: 'Search your current cursor-selected words.',
      },
    ],
  },
  {
    id: 'raycast.diff-checker',
    name: 'Diff Checker',
    description: 'Compare content of two texts',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/diff-checker',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/diff-checker/main/assets/icon.png',
    categories: ['Data', 'Developer Tools'],
    commands: [
      {
        name: 'check-diff',
        title: 'Check Diff',
        description: 'Compare two text snippets line by line',
      },
      {
        name: 'compare-json',
        title: 'Compare JSON',
        description: 'Paste two JSON snippets to see the differences',
      },
    ],
  },
  {
    id: 'raycast.emoji',
    name: 'Emoji Search',
    description: 'Finds emojis and inserts or copies them.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/emoji',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/emoji/main/assets/command-icon.png',
    categories: ['System', 'Media'],
    commands: [
      {
        name: 'emoji',
        title: 'Search Emoji',
        description: 'Finds emojis and inserts or copies them.',
      },
    ],
  },
  {
    id: 'raycast.emoji-kitchen',
    name: 'Emoji Mashups',
    description:
      'Discover and copy Google Emoji Kitchen mashups fast. Browse unique emoji combinations and use them as fun, expressive images in messages and posts. Copy/paste or save.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/emoji-kitchen',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/emoji-kitchen/main/assets/icon.png',
    categories: ['Fun', 'Media'],
    commands: [
      {
        name: 'index',
        title: 'Search Emoji Kitchen',
        description: 'Search for emojis and see their mashups.',
      },
    ],
  },
  {
    id: 'raycast.epoch-to-timestamp',
    name: 'Epoch to Timestamp',
    description:
      'Quickly convert a Unix epoch, in seconds or milliseconds, to a human-readible timestamp.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/epoch-to-timestamp',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/epoch-to-timestamp/main/assets/extension-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'epoch',
        title: 'Epoch',
        description:
          'Convert a Unix epoch in your clipboard, in seconds or milliseconds, to a human-readible timestamp.',
      },
    ],
  },
  {
    id: 'raycast.file-info',
    name: 'File Info',
    description:
      'Quickly view or strip metadata (EXIF, GPS, IPTC, etc.) from selected files for privacy and cleaner sharing. Works on macOS and Windows.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/file-info',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/file-info/main/assets/extension-icon.png',
    categories: ['Productivity', 'Developer Tools', 'System'],
    commands: [
      {
        name: 'show-file-info',
        title: 'Show File Info',
        description: 'Display metadata of the currently selected file',
      },
      {
        name: 'clear-file-metadata',
        title: 'Clear File Metadata',
        description:
          'Strip all embedded metadata (EXIF, GPS, IPTC, XMP, etc.) and system attributes from selected files.',
      },
    ],
  },
  {
    id: 'raycast.file-tree-generator',
    name: 'File Tree Generator',
    description: 'Generates file tree from text',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/file-tree-generator',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/file-tree-generator/main/assets/command-icon.png',
    categories: ['Developer Tools', 'Productivity'],
    commands: [
      { name: 'index', title: 'Create File Tree', description: 'Generates file tree from text' },
    ],
  },
  {
    id: 'raycast.find-website',
    name: 'Find Website',
    description: 'Quickly find the website you are looking for.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/find-website',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/find-website/main/assets/logo.png',
    categories: ['Productivity'],
    commands: [
      {
        name: 'find',
        title: 'Find Website',
        description: 'Quickly find the website you are looking for.',
      },
    ],
  },
  {
    id: 'raycast.fontawesome',
    name: 'Font Awesome',
    description: 'Search Font Awesome icons',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/fontawesome',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/fontawesome/main/assets/command-icon.png',
    categories: ['Design Tools', 'Developer Tools'],
    commands: [
      { name: 'index', title: 'Search Font Awesome', description: 'Search Font Awesome icons' },
    ],
  },
  {
    id: 'raycast.font-converter',
    name: 'Font Converter',
    description: 'Convert fonts to TTF, WOFF, WOFF2, and EOT formats directly from Raycast',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/font-converter',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/font-converter/main/assets/extension-icon.png',
    categories: ['Developer Tools', 'Design Tools'],
    commands: [
      {
        name: 'convert-font',
        title: 'Convert Font',
        description: 'Convert a font file into one or more formats.',
      },
      {
        name: 'generate-font-face-css',
        title: 'Generate @Font-Face CSS',
        description: 'Generate a CSS snippet for the converted font files.',
      },
      {
        name: 'preview-font',
        title: 'Preview Font',
        description: 'Preview any installed or custom font.',
      },
    ],
  },
  {
    id: 'raycast.font-search',
    name: 'Font Search',
    description: 'Search locally installed fonts, as displayed in font book.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/font-search',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/font-search/main/assets/extension-icon.png',
    categories: ['Design Tools', 'Developer Tools'],
    commands: [
      {
        name: 'search-fonts',
        title: 'Search Fonts',
        description: 'Search locally installed fonts',
      },
    ],
  },
  {
    id: 'raycast.format-graphql',
    name: 'Format GraphQL',
    description: 'Formats a GraphQL document',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/format-graphql',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/format-graphql/main/assets/command-icon.png',
    categories: ['Developer Tools', 'Productivity'],
    commands: [
      { name: 'index', title: 'Format GraphQL', description: 'Formats a GraphQL document' },
    ],
  },
  {
    id: 'raycast.geohash-encode-decode',
    name: 'Geohash',
    description: 'Encode coordinates as geohash, or decode a geohash and get useful info',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/geohash-encode-decode',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/geohash-encode-decode/main/assets/command-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'encode-coordinates',
        title: 'Encode as Geohash',
        description:
          "Enter a latitude and longitude and get that point's representation as a geohash for any precision",
      },
      {
        name: 'decode-geohash',
        title: 'Decode Geohash',
        description:
          'Enter a geohash and get insights such as the GeoJSON or WKT Polygon representation, centroid, neighbors, and more',
      },
    ],
  },
  {
    id: 'raycast.hashrate-no',
    name: 'Hashrate',
    description: 'Crypto mining resources via Hashrate.no',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/hashrate-no',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/hashrate-no/main/assets/hashrate-no.png',
    categories: ['Data', 'Finance'],
    commands: [
      { name: 'coins', title: 'Coins', description: 'Crypto Coins with Pricing and Estimates' },
      {
        name: 'gpu_estimates',
        title: 'GPU Estimates',
        description: 'GPU Estimates according to Power Cost ($/kWh)',
      },
    ],
  },
  {
    id: 'raycast.html-colors',
    name: 'HTML Colors',
    description:
      'Search through pallettes of standard HTML colors. Offers basic + extended set. Search by name or browse by looking at the color itself.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/html-colors',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/html-colors/main/assets/icon.png',
    categories: ['Developer Tools', 'Design Tools'],
    commands: [
      {
        name: 'html-color',
        title: 'Search HTML Colors',
        description:
          'Quickly find HTML colors by name, hex, or RGB. Supports fuzzy search with typos and shows both basic and extended color sets.',
      },
    ],
  },
  {
    id: 'raycast.image-base64',
    name: 'Image Base64 Converter',
    description: 'Convert between base64 strings and images easily',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/image-base64',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/image-base64/main/assets/extension-icon.png',
    categories: ['Developer Tools'],
    commands: [
      { name: 'base64-to-image', title: 'Base64 to Image', description: 'Convert base64 to image' },
      { name: 'image-to-base64', title: 'Image to Base64', description: 'Convert image to base64' },
    ],
  },
  {
    id: 'raycast.image-diff-checker',
    name: 'Image Diff Checker',
    description:
      'Image Diff Checker compares two images and highlights the differences. It supports file formats such as JPEG, JPG, PNG, and GIF.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/image-diff-checker',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/image-diff-checker/main/assets/image-diff-checker-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'image-diff-check',
        title: 'Image Diff Check',
        description: 'Compares two images to identify differences.',
      },
    ],
  },
  {
    id: 'raycast.image-host',
    name: 'Image Host',
    description: 'Host an image on imgBB',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/image-host',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/image-host/main/assets/logo.png',
    categories: ['Media'],
    commands: [{ name: 'host-image', title: 'Host Image', description: 'Host an image on imgBB' }],
  },
  {
    id: 'raycast.image-search',
    name: 'Image Web Search',
    description: 'Searches image using Google Image API',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/image-search',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/image-search/main/assets/extension-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'search-images',
        title: 'Search Web Images',
        description: 'Searches Images on the web',
      },
    ],
  },
  {
    id: 'raycast.image-shield',
    name: 'Image Shield',
    description: 'Protect images with advanced encryption and fragmentation.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/image-shield',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/image-shield/main/assets/extension-icon.png',
    categories: ['Security', 'Media'],
    commands: [
      {
        name: 'encrypt-images',
        title: 'Encrypt Images',
        description: 'Encrypt images into secure fragments',
      },
      {
        name: 'decrypt-images',
        title: 'Decrypt Images',
        description: 'Restore original images from the fragments',
      },
    ],
  },
  {
    id: 'raycast.image-to-ascii',
    name: 'Image to Ascii',
    description: 'Convert image to Ascii',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/image-to-ascii',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/image-to-ascii/main/assets/extension-icon.png',
    categories: ['Applications', 'Media'],
    commands: [
      {
        name: 'convert-image-to-ascii',
        title: 'Convert Image to Ascii',
        description: 'Convert image to Ascii',
      },
    ],
  },
  {
    id: 'raycast.ipapi-is',
    name: 'ipapi.is',
    description: 'Lookup IP or ASN via ipapi.is',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/ipapi-is',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/ipapi-is/main/assets/ipapi-is.png',
    categories: ['Data'],
    commands: [
      { name: 'lookup-ip-or-asn', title: 'Lookup IP or ASN', description: 'Lookup IP or ASN' },
    ],
  },
  {
    id: 'raycast.ipcheck-ing',
    name: 'IPCheck',
    description:
      'Show your local and external IPs from multiple sources, look up the location and network behind any IP address, and keep your current IP in the menu bar',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/ipcheck-ing',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/ipcheck-ing/main/assets/ipchecking.png',
    categories: ['Developer Tools'],
    commands: [
      { name: 'main', title: 'Show My IPs', description: 'Show All My IPs' },
      {
        name: 'query-ip',
        title: 'Query IP',
        description: 'Look up the location, ISP and network behind any IP address',
      },
      {
        name: 'menubar',
        title: 'IP in Menu Bar',
        description: 'Keep your current external IP in the menu bar',
      },
    ],
  },
  {
    id: 'raycast.ip-finder',
    name: 'Ip Finder - Network Scanner',
    description:
      'Scan your local network to find assigned IPs and get recommendations for available addresses',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/ip-finder',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/ip-finder/main/assets/command-icon.png',
    categories: ['Developer Tools', 'System'],
    commands: [
      {
        name: 'index',
        title: 'Scan Network',
        description: 'Scan local network for assigned IPs and get recommendations',
      },
    ],
  },
  {
    id: 'raycast.ip-geolocation',
    name: 'IP Geolocation',
    description:
      'Show local and public IPv4/IPv6 address.\nQuery geolocation for any IP address or domain.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/ip-geolocation',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/ip-geolocation/main/assets/extension-icon.png',
    categories: ['Developer Tools', 'Web'],
    commands: [
      {
        name: 'query-ip-geolocation',
        title: 'Query IP Geolocation',
        description: 'Query geolocation for any IP address or domain.',
      },
      {
        name: 'my-ip-geolocation',
        title: 'My IP Geolocation',
        description: 'Show geolocation for local and public IPv4/IPv6 address.',
      },
      { name: 'copy-ip', title: 'Copy IP', description: 'Copy local/public IPv4/IPv6 address.' },
    ],
  },
  {
    id: 'raycast.ip-tools',
    name: 'IP Tools',
    description:
      'This is an extension plugin for converting, validating, and calculating IP subnets. (net, mask, netmask, ip2long, long2ip, cidr, ipv4, ipv6 & geoLocation)',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/ip-tools',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/ip-tools/main/assets/extension_icon.png',
    categories: ['Developer Tools', 'Productivity'],
    commands: [
      {
        name: 'ip2long',
        title: 'Convert IP Address to Number',
        description: 'Convert IPv4 or IPv6 Address to Number',
      },
      {
        name: 'long2ip',
        title: 'Convert Number to IP Address',
        description: 'Convert Number to IPv4 or IPv6',
      },
      {
        name: 'ip2BinHex',
        title: 'Convert IP Address to Bin and Hex',
        description: 'Convert IPv4 or IPv6 Address to binary and hexadecimal',
      },
      {
        name: 'parseCIDR',
        title: 'Convert CIDR to IP Range',
        description: 'Convert CIDR to IPv4 or IPv6 Range',
      },
      {
        name: 'isValidIP',
        title: 'Verify IP Address Is Valid',
        description: 'Verify if the IP address Is valid',
      },
      {
        name: 'isValidCIDR',
        title: 'Verify CIDR Is Valid',
        description: 'Verify if the CIDR is valid',
      },
      {
        name: 'toIPv6Format',
        title: 'Converts IPv4 to IPv6 Address',
        description: 'Converts IPv4 address to IPv6 address',
      },
      {
        name: 'toIPv6Expand',
        title: 'Converts IPv6 to Expanded or Compressed',
        description: 'Converts IPv6 to expanded or compressed format',
      },
      {
        name: 'geoLocation',
        title: 'Query IP Geolocation Information',
        description: 'Query IPv4 or Ipv6 Geolocation Information',
      },
    ],
  },
  {
    id: 'raycast.json2ts',
    name: 'Json2TS',
    description: 'Generate TypeScript interfaces from JSON',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/json2ts',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/json2ts/main/assets/command-icon.png',
    categories: ['Developer Tools'],
    commands: [
      { name: 'index', title: 'Json2ts', description: 'Generate TypeScript interfaces from JSON' },
    ],
  },
  {
    id: 'raycast.json-editor',
    name: 'JSON Editor',
    description: 'Format, filter, and convert JSON directly in Tezbar.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/json-editor',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/json-editor/main/assets/icon.svg',
    categories: ['Productivity', 'Developer Tools', 'Data'],
    commands: [
      {
        name: 'edit-json',
        title: 'Edit JSON',
        description: 'Format, filter, minify, escape, and convert JSON.',
      },
    ],
  },
  {
    id: 'raycast.json-format',
    name: 'Format JSON',
    description:
      'Formats a JSON file with a selected indentation. If the JSON is stringified, it will be parsed before formatting.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/json-format',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/json-format/main/assets/command-icon.png',
    categories: ['Data', 'Developer Tools'],
    commands: [
      {
        name: 'formatClipboard',
        title: 'Format Clipboard JSON',
        description:
          'Formats a JSON/JS Object string stored in clipboard and copies/pastes it back',
      },
      {
        name: 'formatSelection',
        title: 'Format Selected JSON',
        description:
          'Formats a JSON/JS Object string selected in the foremost editor and copies/pastes it back',
      },
      {
        name: 'formatSelectionAndShow',
        title: 'Format Selected JSON and Show',
        description:
          'Formats a JSON/JS Object string selected in the foremost editor and shows it in the viewer',
      },
      { name: 'index', title: 'Format JSON', description: 'Formats a JSON/JS Object' },
      {
        name: 'formatJsonLines',
        title: 'Format Array of JSON to JSONLines',
        description: 'Formats an array of JSON/JS Object to JSONLines',
      },
      {
        name: 'formatToJsonValue',
        title: 'Format Text into Valid JSON Value',
        description: 'Formats text into a valid JSON value with double quotes and escapes',
      },
    ],
  },
  {
    id: 'raycast.json-resume',
    name: 'JSON Resume',
    description: 'Parse JSON Resume',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/json-resume',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/json-resume/main/assets/extension-icon.png',
    categories: ['Productivity', 'Communication'],
    commands: [
      {
        name: 'open-resume',
        title: 'Open Resume',
        description: 'Open a resume by providing a JSON resume URL',
      },
      {
        name: 'search-resumes',
        title: 'View Saved Resumes',
        description: 'View and manage saved resumes',
      },
    ],
  },
  {
    id: 'raycast.json-to-toon-converter',
    name: 'JSON to TOON Converter',
    description: 'Convert JSON data into TOON format instantly.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/json-to-toon-converter',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/json-to-toon-converter/main/assets/extension-icon.png',
    categories: ['Developer Tools'],
    commands: [
      { name: 'convert-json', title: 'Convert JSON', description: 'Convert JSON to TOON format' },
    ],
  },
  {
    id: 'raycast.kind-words',
    name: 'Kind Words',
    description: 'Random compliments and gratitude prompts to lift the mood.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/kind-words',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/kind-words/main/assets/extension-icon.png',
    categories: ['Fun'],
    commands: [
      {
        name: 'random-compliment',
        title: 'Random Compliment',
        description: 'Browse compliments with one preselected at random; filter by tone.',
      },
      {
        name: 'quick-compliment',
        title: 'Quick Compliment',
        description: 'Copy a random compliment instantly.',
      },
      {
        name: 'gratitude-note',
        title: 'Gratitude Note',
        description: 'Browse gratitude prompts with usage notes and opener examples.',
      },
    ],
  },
  {
    id: 'raycast.language-tool',
    name: 'Language Tool - Spell & Grammar Checker',
    description:
      'Raycast extension that provides instant spelling and grammar correction using LanguageTool. Type any text, get real-time suggestions, and copy the improved version. Ideal for developers, writers, and anyone who wants fast, high-quality text corrections in multiple languages.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/language-tool',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/language-tool/main/assets/extension-icon.png',
    categories: ['Communication'],
    commands: [
      {
        name: 'check-text',
        title: 'Check Text',
        description: 'Check a text with LanguageTool for possible style and grammar issues.',
      },
      {
        name: 'check-text-instant',
        title: 'Check Text Instant',
        description:
          'Check a text with LanguageTool using the clipboard content and paste the result instantly. The language is detected automatically.',
      },
    ],
  },
  {
    id: 'raycast.latex-math-symbols',
    name: 'LateX Math Symbols',
    description: 'Search and copy LateX Math Symbols',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/latex-math-symbols',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/latex-math-symbols/main/assets/icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'index',
        title: 'Search LateX Symbols',
        description: 'Search through a library of latex math symbols from amsmath',
      },
    ],
  },
  {
    id: 'raycast.list-randomizer',
    name: 'List Randomizer',
    description: 'Randomizes a list of items',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/list-randomizer',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/list-randomizer/main/assets/command-icon.png',
    categories: ['Data'],
    commands: [
      {
        name: 'index',
        title: 'Randomize List',
        description: 'Add items to a list and randomize it',
      },
    ],
  },
  {
    id: 'raycast.lorem-ipsum',
    name: 'Lorem Ipsum',
    description: 'Generate placeholder content',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/lorem-ipsum',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/lorem-ipsum/main/assets/paragraph-icon.png',
    categories: ['Data', 'Design Tools', 'Developer Tools', 'Web'],
    commands: [
      {
        name: 'paragraphs',
        title: 'Generate Paragraphs',
        description: 'Generate random paragraphs and copy them to the clipboard',
      },
      {
        name: 'sentences',
        title: 'Generate Sentences',
        description: 'Generate random sentences and copy them to the clipboard',
      },
      {
        name: 'words',
        title: 'Generate Words',
        description: 'Generate random words and copy them to the clipboard',
      },
      {
        name: 'ai-generate',
        title: 'Generate with AI',
        description: 'Use AI to generate some placeholder text',
      },
    ],
  },
  {
    id: 'raycast.markdown-image-to-html',
    name: 'Markdown Image to HTML',
    description:
      'Transform a Markdown Image to HTML. This allows us to fix image size in Markdown base files.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/markdown-image-to-html',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/markdown-image-to-html/main/assets/command-icon.png',
    categories: ['Developer Tools', 'Productivity'],
    commands: [
      {
        name: 'index',
        title: 'Markdown Image to HTML',
        description:
          'Transform a Markdown Image to HTML. This allows us to fix image size in Markdown base files.',
      },
    ],
  },
  {
    id: 'raycast.markdown-preview',
    name: 'Markdown Preview',
    description: 'A simple way to preview markdown content',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/markdown-preview',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/markdown-preview/main/assets/md-preview-icon.png',
    categories: ['Productivity', 'Web', 'Developer Tools', 'Documentation'],
    commands: [
      {
        name: 'preview-markdown',
        title: 'Preview Markdown',
        description: 'Write and preview Markdown content with real-time rendering',
      },
    ],
  },
  {
    id: 'raycast.markdown-reference',
    name: 'Markdown Reference',
    description: 'Markdown documentation at your fingertips',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/markdown-reference',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/markdown-reference/main/assets/logo.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'markdown-reference',
        title: 'Search Markdown Syntax',
        description: 'Quickly search up how to format with Markdown',
      },
    ],
  },
  {
    id: 'raycast.markdown-styler',
    name: 'Markdown Styler',
    description: 'Convert Markdown to styled HTML with inline CSS for content platforms',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/markdown-styler',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/markdown-styler/main/assets/command-icon.png',
    categories: ['Productivity'],
    commands: [
      {
        name: 'mp-style-markdown',
        title: 'Style Markdown',
        description: 'Convert Markdown to styled HTML with inline CSS for content platforms',
      },
    ],
  },
  {
    id: 'raycast.markdown-table-generator',
    name: 'Markdown Table Generator',
    description:
      'Specify your width and height, and receive a markdown table that you can input your own values into.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/markdown-table-generator',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/markdown-table-generator/main/assets/command-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'generate-markdown-table',
        title: 'Generate Markdown Table',
        description:
          'Based on the inputted integer arguments, number of rows and number of columns, you will receive a markdown table.',
      },
    ],
  },
  {
    id: 'raycast.markdown-this',
    name: 'Markdown This',
    description: 'Convert Markdown text in clipboard to HTML',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/markdown-this',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/markdown-this/main/assets/command-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'index',
        title: 'Convert to HTML',
        description: 'Convert markdown text in clipboard to HTML',
      },
    ],
  },
  {
    id: 'raycast.markdown-to-jira',
    name: 'Markdown to Jira',
    description: 'Format a Markdown text into Jira markup.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/markdown-to-jira',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/markdown-to-jira/main/assets/icon.png',
    categories: ['Documentation', 'Productivity'],
    commands: [
      {
        name: 'markdown-to-jira',
        title: 'Markdown to Jira',
        description: 'Format a Markdown text into Jira markup.',
      },
    ],
  },
  {
    id: 'raycast.mastodon-search',
    name: 'Mastodon Search',
    description: 'Search for People or Hashtags on Mastodon.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/mastodon-search',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/mastodon-search/main/assets/command-icon.png',
    categories: ['Communication', 'Web'],
    commands: [
      {
        name: 'search',
        title: 'Search Mastodon',
        description: 'Search Mastodon for people or hashtags.',
      },
    ],
  },
  {
    id: 'raycast.modify-hash',
    name: 'Modify Hash',
    description:
      'Batch modify the hash of media files, which can be simply used for uploading online disk to prevent censorship.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/modify-hash',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/modify-hash/main/assets/hasher.png',
    categories: ['Developer Tools', 'Productivity'],
    commands: [
      {
        name: 'modifyHash',
        title: 'Modify Hash',
        description: 'Batch modify the hash of media files.',
      },
      {
        name: 'restoreHash',
        title: 'Restore Hash',
        description: 'Batch restore the hash of media files.',
      },
      {
        name: 'zipCompress',
        title: 'Zip Compress',
        description: 'Compress files to a portable zip archive.',
      },
      {
        name: 'zipExtract',
        title: 'Zip Extract',
        description: 'Extract files from a portable zip archive.',
      },
    ],
  },
  {
    id: 'raycast.node-release-notes',
    name: 'Node Release Notes',
    description:
      'Quickly browse and access detailed information of Node.js versions. Find release notes, publication dates, and documentation links for any Node.js version. Ideal for developers and sysadmins to stay informed about Node.js updates and ensure compatibility with their projects.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/node-release-notes',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/node-release-notes/main/assets/app-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'index',
        title: 'Node Versions',
        description: 'Quickly browse and access detailed information of Node.js versions',
      },
    ],
  },
  {
    id: 'raycast.oklch-color-converter',
    name: 'OKLCH Color Converter',
    description: 'Convert colors between formats with wide gamut support',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/oklch-color-converter',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/oklch-color-converter/main/icon.png',
    categories: ['Design Tools'],
    commands: [
      {
        name: 'convert-color',
        title: 'Convert Color',
        description: 'Convert colors between different color spaces and formats',
      },
    ],
  },
  {
    id: 'raycast.onelook-thesaurus',
    name: 'OneLook Thesaurus',
    description:
      "A powerful English thesaurus and brainstorming tool that lets you describe what you're looking for in plain terms.",
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/onelook-thesaurus',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/onelook-thesaurus/main/assets/command-icon.png',
    categories: ['Productivity'],
    commands: [
      {
        name: 'search',
        title: 'Search',
        description:
          'Enter a word, phrase, description, or pattern above to find synonyms, related words, and more.',
      },
    ],
  },
  {
    id: 'raycast.one-time-password',
    name: 'One Time Password',
    description: 'Generate time-based one-time passwords',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/one-time-password',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/one-time-password/main/assets/icon.png',
    categories: ['Security', 'Productivity'],
    commands: [
      {
        name: 'one-time-password',
        title: 'One Time Password',
        description: 'Generate time-based one-time passwords',
      },
      {
        name: 'paste-recent-otp',
        title: 'Paste Recent OTP',
        description: 'Paste the OTP code from the most recently used account into the active app',
      },
    ],
  },
  {
    id: 'raycast.password-generator',
    name: 'Password Generator',
    description: 'Generate secure random and memorable passwords.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/password-generator',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/password-generator/main/assets/icon.svg',
    categories: ['Productivity', 'Developer Tools', 'Security'],
    commands: [
      {
        name: 'generate-random-password',
        title: 'Generate Random Password',
        description: 'Generate a secure password with configurable character groups.',
      },
      {
        name: 'copy-random-password',
        title: 'Copy Random Password',
        description: 'Generate a random password and copy it immediately.',
      },
      {
        name: 'generate-memorable-password',
        title: 'Generate Memorable Password',
        description: 'Generate a passphrase from words, numbers, and symbols.',
      },
    ],
  },
  {
    id: 'raycast.password-strength',
    name: 'Password Strength',
    description: 'Check your passwords',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/password-strength',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/password-strength/main/assets/extension-icon.png',
    categories: ['Web', 'Security'],
    commands: [
      {
        name: 'check-password-strength',
        title: 'Check Password Strength',
        description: 'Check the strength of your password',
      },
    ],
  },
  {
    id: 'raycast.paste-as-plain-text',
    name: 'Paste as Plain Text',
    description: 'Paste text from the clipboard as any format.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/paste-as-plain-text',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/paste-as-plain-text/main/assets/paste-icon.png',
    categories: ['Developer Tools', 'Productivity', 'System'],
    commands: [
      {
        name: 'paste-as-plain-text',
        title: 'Paste as',
        description: 'Paste text from the clipboard as any format.',
      },
    ],
  },
  {
    id: 'raycast.qrcode-generator',
    name: 'QR Code Generator',
    description: 'Generate QR codes from text or URLs.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/qrcode-generator',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/qrcode-generator/main/assets/command-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'index',
        title: 'Generate QR Code',
        description: 'Generate a QR code from text or URL',
      },
      {
        name: 'clipboard',
        title: 'Generate QR Code from Clipboard',
        description: 'Generate a QR code from clipboard content',
      },
      {
        name: 'selection',
        title: 'Generate QR Code from Selection',
        description: 'Generate a QR code from selection content',
      },
    ],
  },
  {
    id: 'raycast.random-data-generator',
    name: 'Random Data Generator',
    description: 'Generate random data using Faker library',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/random-data-generator',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/random-data-generator/main/assets/random.png',
    categories: ['Data', 'Developer Tools'],
    commands: [
      {
        name: 'index',
        title: 'Generate Random Data',
        description: 'Generate random data using Faker library',
      },
      {
        name: 'open-quicklink',
        title: 'Open Quicklink',
        description: 'Used internally to run quicklinks from the generate command.',
      },
    ],
  },
  {
    id: 'raycast.regex-tester',
    name: 'Regex Tester',
    description: 'Preview and test regular expressions',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/regex-tester',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/regex-tester/main/assets/command-icon.png',
    categories: ['Developer Tools', 'Productivity'],
    commands: [
      { name: 'index', title: 'Test Regex', description: 'Preview and test regular expressions' },
    ],
  },
  {
    id: 'raycast.text-differ',
    name: 'Text Differ',
    description: 'Compare two selected text files directly in Tezbar',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/text-differ',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/text-differ/main/assets/command-icon.png',
    categories: ['Applications', 'Developer Tools'],
    commands: [
      {
        name: 'open-with-text-differ',
        title: 'Compare Selected Files',
        description: 'Show a line-by-line diff for two files selected in Finder or File Explorer',
      },
    ],
  },
  {
    id: 'raycast.text-rewrap',
    name: 'Text Rewrap',
    description: 'Rewraps Text at a given width',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/text-rewrap',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/text-rewrap/main/assets/text-rewrap.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'text-rewrap-form',
        title: 'Text Rewrap from Form',
        description: 'Rewraps Text provided in the form at the chosen width',
      },
      {
        name: 'text-rewrap-clipboard',
        title: 'Text Rewrap from Clipboard',
        description: 'Rewraps text from clipboard at the chosen width',
      },
      {
        name: 'text-rewrap-selection',
        title: 'Text Rewrap from Selection',
        description: 'Rewraps text from currently highlighted selection at the chosen width',
      },
    ],
  },
  {
    id: 'raycast.timezone-converter',
    name: 'Timezone Converter',
    description: 'Converts any time to any timezone',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/timezone-converter',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/timezone-converter/main/assets/command-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'index',
        title: 'Timezone Converter',
        description: 'Converts any time to any timezone',
      },
    ],
  },
  {
    id: 'raycast.unicode-symbols',
    name: 'Unicode Symbols Search',
    description: 'Browse and copy-paste common Unicode symbols.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/unicode-symbols',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/unicode-symbols/main/assets/command-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'index',
        title: 'Search Unicode Symbol',
        description: 'Browse and copy-paste common Unicode symbols.',
      },
    ],
  },
  {
    id: 'raycast.uuid-generator',
    name: 'UUID Generator',
    description: 'A quick way to generate UUIDs without opening the browser',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/uuid-generator',
    iconUrl:
      'https://raw.githubusercontent.com/almatkai/uuid-generator/main/assets/command-icon.png',
    categories: ['Developer Tools'],
    commands: [
      {
        name: 'generate',
        title: 'Generate UUIDs',
        description: 'Copy generated UUIDs to the clipboard',
      },
      {
        name: 'generateV5',
        title: 'Generate V5 UUIDs',
        description: 'Copy generated UUIDs V5 to the clipboard',
      },
      {
        name: 'generateV7',
        title: 'Generate V7 UUIDs',
        description: 'Copy generated UUIDs V7 to the clipboard',
      },
      {
        name: 'generateUlid',
        title: 'Generate ULIDs',
        description: 'Copy generated ULIDs to the clipboard',
      },
      {
        name: 'generateTypeID',
        title: 'Generate TypeIDs',
        description: 'Copy generated TypeIDs to the clipboard',
      },
      {
        name: 'parseTypeID',
        title: 'Parse TypeIDs',
        description: 'Parse TypeID and copy resulting UUID to the clipboard',
      },
      {
        name: 'formatUuid',
        title: 'Format UUID',
        description:
          'Format UUID without dashes into standard UUID format and copy to the clipboard',
      },
      {
        name: 'packTypeID',
        title: 'Pack TypeID',
        description: 'Convert a UUID to a packed Type ID with a suffix',
      },
      { name: 'viewHistory', title: 'View History', description: 'View the UUID history' },
    ],
  },
  {
    id: 'raycast.word-count',
    name: 'Word Count',
    description: 'Count characters, words, sentences, and paragraphs in text.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/word-count',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/word-count/main/assets/word-count.png',
    categories: ['Productivity'],
    commands: [
      {
        name: 'count',
        title: 'Count',
        description: 'Count characters, words, sentences, and paragraphs in text.',
      },
      {
        name: 'count-screenshot',
        title: 'Count Clipboard Text',
        description: 'Count words, characters, sentences, and paragraphs in clipboard text.',
      },
      {
        name: 'count-overlay',
        title: 'Word Count in Overlay',
        description: 'Show word count in an overlay on top of other applications.',
      },
    ],
  },
  {
    id: 'raycast.world-clock',
    name: 'World Clock',
    description: 'Query the current time of a region, ip or domain.',
    author: 'almatkai',
    owner: 'almatkai',
    version: '0.1.0',
    repository: 'https://github.com/almatkai/world-clock',
    iconUrl: 'https://raw.githubusercontent.com/almatkai/world-clock/main/assets/command-icon.png',
    categories: ['Data', 'Other'],
    commands: [
      {
        name: 'query-world-time',
        title: 'Query World Time',
        description: 'Query the current time for a timezone with region.',
      },
      {
        name: 'query-world-time-menu-bar',
        title: 'Menubar World Time',
        description: 'Query the current time of starred timezones from the menu bar.',
      },
      {
        name: 'query-ip-time',
        title: 'Query IP Time',
        description: 'Query the current time based on IP or domain.',
      },
    ],
  },
  // Additional Tezbar-maintained extensions are appended below.
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
