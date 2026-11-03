/**
 * Chat title and icon generation.
 *
 * Provides heuristic rule-based titles and emoji icons instantly (offline/synchronous),
 * and defines the contract for model-generated titles and icons.
 */

export function isEmoji(str?: string | null): boolean {
  if (!str) return false
  const trimmed = str.trim()
  if (!trimmed) return false
  return /\p{Extended_Pictographic}/u.test(trimmed)
}

export function chatIconToSquircleKind(icon?: string | null): string {
  if (!icon) return 'chat'
  const trimmed = icon.trim().toLowerCase()
  switch (trimmed) {
    case '⚡':
    case 'code':
    case 'coding':
    case 'dev':
    case 'mcp':
    case 'script':
    case 'snippets':
      return 'snippets'
    case '🔑':
    case 'key':
    case 'api-key':
    case 'apikey':
    case 'token':
    case 'auth':
    case 'password':
      return 'key'
    case '💻':
    case 'cpu':
    case 'hardware':
    case 'system':
    case 'mac':
    case 'pc':
      return 'cpu'
    case '⚽':
    case '🏀':
    case '🎾':
    case 'sport':
    case 'sports':
    case 'football':
    case 'soccer':
    case 'messi':
    case 'ronaldo':
    case 'trophy':
      return 'trophy'
    case '📝':
    case '✍️':
    case 'edit':
    case 'pencil':
    case 'draft':
    case 'notes':
    case 'note':
    case 'writing':
      return 'edit'
    case '🤖':
    case 'ai':
    case 'llm':
    case 'sparkles':
      return 'ai'
    case '📋':
    case 'clipboard':
      return 'clipboard'
    case '☀️':
    case 'weather':
    case 'sun':
      return 'weather'
    case '🎵':
    case 'music':
    case 'audio':
      return 'audio'
    case '💰':
    case 'finance':
    case 'crypto':
    case 'money':
      return 'brew'
    case '🔍':
    case 'search':
    case 'web':
      return 'search'
    case '⚙️':
    case 'settings':
    case 'config':
      return 'settings'
    case '📁':
    case 'folder':
    case 'files':
      return 'finder'
    case '💬':
    case 'chat':
      return 'chat'
    default:
      return trimmed || 'chat'
  }
}

function wordMatch(text: string, pattern: string): boolean {
  const regex = new RegExp(`(?:^|[^\\p{L}\\p{N}])(?:${pattern})(?:[^\\p{L}\\p{N}]|$)`, 'iu')
  return regex.test(text)
}

function cleanPromptSnippet(prompt: string): string {
  const firstLine = prompt.split('\n').find((l) => l.trim()) ?? ''
  let cleaned = firstLine.trim()

  const prefixRegex =
    /^(can you|could you|please|tell me|who is|who was|what is|what was|what are|where is|when is|when did|when was|how to|how do i|how does|why is|why does|explain|help me with|i need to|look up|подскажи пожалуйста|расскажи пожалуйста|подскажи|расскажи|пожалуйста|кто такой|кто такая|что такое|что за|как сделать|как настроить|как вообще|как|почему|зачем|объясни|найди|когда вышел|когда выйдет|когда)\s+/i

  let previous = ''
  while (previous !== cleaned) {
    previous = cleaned
    cleaned = cleaned.replace(prefixRegex, '')
  }

  // Remove trailing punctuation first so trailing query clauses match
  cleaned = cleaned.replace(/[\?\!\.\:\;]+$/g, '').trim()

  // Strip common trailing query clauses
  cleaned = cleaned
    .replace(
      /\s+(when it came out|when did it come out|when was it released|когда вышел|когда выйдет|как вообще генерить|как генерить|как сделать|как настроить|как использовать)$/i,
      ''
    )
    .replace(/[\?\!\.\:\;]+$/g, '')
    .trim()

  if (!cleaned) return ''

  // Capitalize first letter
  cleaned = cleaned.charAt(0).toUpperCase() + cleaned.slice(1)

  if (cleaned.length > 36) {
    const truncated = cleaned.slice(0, 36)
    const lastSpace = truncated.lastIndexOf(' ')
    return (lastSpace > 20 ? truncated.slice(0, lastSpace) : truncated).trim() + '…'
  }
  return cleaned
}

export function heuristicChatMeta(
  prompt: string,
  answer?: string
): { title: string; icon: string } {
  const combined = `${prompt} ${answer || ''}`.toLowerCase()

  // Specific high-frequency topic heuristics
  if (wordMatch(combined, 'messi|месси')) {
    return { title: 'Lionel Messi', icon: 'trophy' }
  }
  if (wordMatch(combined, 'ronaldo|роналду|криштиану')) {
    return { title: 'Cristiano Ronaldo', icon: 'trophy' }
  }
  if (wordMatch(combined, 'cpu|processor|процессор|память|ram|htop|activity monitor|нагрузк[а-я]*')) {
    if (wordMatch(combined, 'compiler|build|compil[a-z]*|idle|rustc|clang|gcc')) {
      return { title: 'CPU Usage & Compiler Memory', icon: 'cpu' }
    }
    return { title: 'CPU & Process Diagnostics', icon: 'cpu' }
  }
  if (wordMatch(combined, 'api key|apikey|api-key|токен|token|auth|секрет|secret')) {
    return { title: cleanPromptSnippet(prompt) || 'API Key & Auth', icon: 'key' }
  }
  if (wordMatch(combined, 'mcp')) {
    const raw = cleanPromptSnippet(prompt)
    const title = raw ? raw.replace(/\bversion\b/gi, 'Version') : 'MCP Protocol'
    return { title, icon: 'snippets' }
  }

  // General keyword categories
  let icon = 'chat'
  if (wordMatch(combined, 'football|soccer|fifa|champions league|футбол')) {
    icon = 'trophy'
  } else if (wordMatch(combined, 'nba|basketball|баскетбол')) {
    icon = 'sports'
  } else if (wordMatch(combined, 'tennis|теннис')) {
    icon = 'sports'
  } else if (wordMatch(combined, 'rust|python|typescript|javascript|golang|c\\+\\+|react|vue|node|bun|docker|k8s|git|github|api|backend|frontend|mcp|кодинг|баг|ошибк[а-я]*')) {
    icon = 'snippets'
  } else if (wordMatch(combined, 'cpu|mac|gpu|ram|terminal|bash|zsh|shell|linux|windows|диск|ssd|памят[а-я]*|процесс[а-я]*')) {
    icon = 'cpu'
  } else if (wordMatch(combined, 'weather|rain|snow|temperature|погода|дождь|снег|температур[а-я]*')) {
    icon = 'weather'
  } else if (wordMatch(combined, 'music|song|album|spotify|artist|песн[а-я]*|музык[а-я]*|трек[а-я]*')) {
    icon = 'audio'
  } else if (wordMatch(combined, 'crypto|bitcoin|btc|eth|finance|stock|dollar|валют[а-я]*|деньг[а-я]*|биткоин|курс')) {
    icon = 'brew'
  } else if (wordMatch(combined, 'ai|gpt|llm|claude|gemini|deepseek|ollama|нейросет[а-я]*|модел[а-я]*')) {
    icon = 'ai'
  } else if (wordMatch(combined, 'write|essay|summary|draft|article|письм[а-я]*|стать[а-я]*|сочинени[а-я]*|пересказ')) {
    icon = 'edit'
  } else if (wordMatch(combined, 'translate|english|russian|spanish|перевод[а-я]*|язык[а-я]*')) {
    icon = 'finder'
  } else if (wordMatch(combined, 'movie|film|cinema|фильм[а-я]*|кино|сериал[а-я]*')) {
    icon = 'picture'
  } else if (wordMatch(combined, 'recipe|food|cooking|coffee|еда|рецепт[а-я]*|кофе')) {
    icon = 'coffee'
  } else if (wordMatch(combined, 'search|find|where|поиск|найди')) {
    icon = 'search'
  }

  const cleanedTitle = cleanPromptSnippet(prompt)
  const title = cleanedTitle || 'New chat'

  return { title, icon }
}
