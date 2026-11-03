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
    return { title: 'Lionel Messi', icon: '⚽' }
  }
  if (wordMatch(combined, 'ronaldo|роналду|криштиану')) {
    return { title: 'Cristiano Ronaldo', icon: '⚽' }
  }
  if (wordMatch(combined, 'cpu|processor|процессор|память|ram|htop|activity monitor|нагрузк[а-я]*')) {
    if (wordMatch(combined, 'compiler|build|compil[a-z]*|idle|rustc|clang|gcc')) {
      return { title: 'CPU Usage & Compiler Memory', icon: '💻' }
    }
    return { title: 'CPU & Process Diagnostics', icon: '💻' }
  }
  if (wordMatch(combined, 'api key|apikey|api-key|токен|token|auth|секрет|secret')) {
    return { title: cleanPromptSnippet(prompt) || 'API Key & Auth', icon: '🔑' }
  }
  if (wordMatch(combined, 'mcp')) {
    const raw = cleanPromptSnippet(prompt)
    const title = raw ? raw.replace(/\bversion\b/gi, 'Version') : 'MCP Protocol'
    return { title, icon: '⚡' }
  }

  // General keyword categories
  let icon = '💬'
  if (wordMatch(combined, 'football|soccer|fifa|champions league|футбол')) {
    icon = '⚽'
  } else if (wordMatch(combined, 'nba|basketball|баскетбол')) {
    icon = '🏀'
  } else if (wordMatch(combined, 'tennis|теннис')) {
    icon = '🎾'
  } else if (wordMatch(combined, 'rust|python|typescript|javascript|golang|c\\+\\+|react|vue|node|bun|docker|k8s|git|github|api|backend|frontend|mcp|кодинг|баг|ошибк[а-я]*')) {
    icon = '⚡'
  } else if (wordMatch(combined, 'cpu|mac|gpu|ram|terminal|bash|zsh|shell|linux|windows|диск|ssd|памят[а-я]*|процесс[а-я]*')) {
    icon = '💻'
  } else if (wordMatch(combined, 'weather|rain|snow|temperature|погода|дождь|снег|температур[а-я]*')) {
    icon = '☀️'
  } else if (wordMatch(combined, 'music|song|album|spotify|artist|песн[а-я]*|музык[а-я]*|трек[а-я]*')) {
    icon = '🎵'
  } else if (wordMatch(combined, 'crypto|bitcoin|btc|eth|finance|stock|dollar|валют[а-я]*|деньг[а-я]*|биткоин|курс')) {
    icon = '💰'
  } else if (wordMatch(combined, 'ai|gpt|llm|claude|gemini|deepseek|ollama|нейросет[а-я]*|модел[а-я]*')) {
    icon = '🤖'
  } else if (wordMatch(combined, 'write|essay|summary|draft|article|письм[а-я]*|стать[а-я]*|сочинени[а-я]*|пересказ')) {
    icon = '📝'
  } else if (wordMatch(combined, 'translate|english|russian|spanish|перевод[а-я]*|язык[а-я]*')) {
    icon = '🌐'
  } else if (wordMatch(combined, 'movie|film|cinema|фильм[а-я]*|кино|сериал[а-я]*')) {
    icon = '🎬'
  } else if (wordMatch(combined, 'recipe|food|cooking|coffee|еда|рецепт[а-я]*|кофе')) {
    icon = '☕'
  } else if (wordMatch(combined, 'search|find|where|поиск|найди')) {
    icon = '🔍'
  }

  const cleanedTitle = cleanPromptSnippet(prompt)
  const title = cleanedTitle || 'New chat'

  return { title, icon }
}
