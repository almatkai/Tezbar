import { describe, expect, it } from 'vitest'
import { parseTopOutput } from './service'

describe('system stats parsing', () => {
  it('extracts CPU, process, load, and memory values from top', () => {
    const parsed = parseTopOutput(`Processes: 670 total, 23 running, 645 sleeping
Load Avg: 6.11, 4.69, 4.60
CPU usage: 35.4% user, 37.82% sys, 27.13% idle
PhysMem: 23G used (4045M wired, 8788M compressor), 349M unused.`)

    expect(parsed.cpu).toEqual({
      userPercent: 35.4,
      systemPercent: 37.82,
      idlePercent: 27.13,
      usagePercent: 73.22,
      loadAverage: [6.11, 4.69, 4.6],
    })
    expect(parsed.processes).toEqual({ total: 670, running: 23 })
    expect(parsed.memory.usedBytes).toBe(23 * 1024 ** 3)
    expect(parsed.memory.freeBytes).toBe(349 * 1024 ** 2)
  })
})
