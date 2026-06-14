import { useEffect, useState } from 'react'
import {
  INITIAL_DEFAULT_TARGET,
  getPinnedDefault,
  getPreferredDefaultTarget,
  listTargetUsage,
  setPinnedDefault,
} from './currency/currencyPreferences'
import { CURRENCY_SYNONYMS } from './currency/currencySynonyms'
import { Section } from './ui/primitives'

const AUTO_VALUE = '__auto__'

/**
 * Deduplicated list of ISO codes known to the fuzzy synonym table —
 * used to populate the Settings dropdown so the user sees everything
 * Tezbar can parse, not only what they've already used.
 */
function buildAllCodes(): string[] {
  const set = new Set<string>()
  for (const row of CURRENCY_SYNONYMS) {
    set.add(row.code)
  }
  return Array.from(set).sort()
}

/**
 * Currency preferences panel — placed inside Settings. The dropdown is
 * ordered so the user's most-used targets come first, matching how the
 * launcher picks its implicit default.
 */
export function CurrencySettings(): JSX.Element {
  const [pinned, setPinned] = useState<string | null>(null)
  const [effective, setEffective] = useState<string>(INITIAL_DEFAULT_TARGET)
  const [usage, setUsage] = useState<Array<{ code: string; count: number }>>([])

  const refresh = (): void => {
    setPinned(getPinnedDefault())
    setEffective(getPreferredDefaultTarget())
    setUsage(listTargetUsage().map(({ code, count }) => ({ code, count })))
  }

  useEffect(() => {
    refresh()
  }, [])

  const allCodes = buildAllCodes()
  const ordered: string[] = []
  for (const row of usage) {
    if (!ordered.includes(row.code)) ordered.push(row.code)
  }
  for (const code of allCodes) {
    if (!ordered.includes(code)) ordered.push(code)
  }

  const usageCount = (code: string): number => usage.find((u) => u.code === code)?.count ?? 0

  const onChange = (value: string): void => {
    if (value === AUTO_VALUE) {
      setPinnedDefault(null)
    } else {
      setPinnedDefault(value)
    }
    refresh()
  }

  return (
    <Section
      title="Currency conversion"
      description={
        <>
          Default target currency when your query omits one (e.g. <code className="font-mono text-ink-2">1$</code>).
          The order below reflects how often you use each currency.
        </>
      }
    >
      <div className="mt-2 flex items-center gap-2">
        <select
          id="currency-default-target"
          value={pinned ?? AUTO_VALUE}
          onChange={(event) => onChange(event.target.value)}
          className="glass-field max-w-[280px]"
        >
          <option value={AUTO_VALUE}>Auto — most used ({effective})</option>
          {ordered.map((code) => {
            const count = usageCount(code)
            return (
              <option key={code} value={code}>
                {code}
                {count > 0 ? ` · used ${count}×` : ''}
              </option>
            )
          })}
        </select>
        <span className="text-[12px] text-ink-3">
          {pinned ? `Pinned to ${pinned}` : `Auto: ${effective}`}
        </span>
      </div>
    </Section>
  )
}
