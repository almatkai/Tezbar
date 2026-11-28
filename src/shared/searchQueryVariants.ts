// Physical QWERTY / Russian ЙЦУКЕН keys, not phonetic transliteration.
const ENGLISH_KEYS = "`qwertyuiop[]asdfghjkl;'zxcvbnm,."
const RUSSIAN_KEYS = 'ёйцукенгшщзхъфывапролджэячсмитьбю'

function remapKeys(query: string, from: string, to: string): string {
  return Array.from(query, (char) => {
    const index = from.indexOf(char)
    return index < 0 ? char : to[index]
  }).join('')
}

/** Keep literal input first: layout corrections are additional search candidates only. */
export function searchQueryVariants(query: string): string[] {
  const literal = query.trim().toLowerCase()
  if (!literal) return []

  const variants = new Set([literal])
  if (/[а-яё]/u.test(literal)) {
    variants.add(remapKeys(literal, RUSSIAN_KEYS, ENGLISH_KEYS))
  }
  if (/[a-z]/.test(literal)) {
    variants.add(remapKeys(literal, ENGLISH_KEYS, RUSSIAN_KEYS))
  }
  return Array.from(variants)
}
