import { describe, expect, it } from 'vitest'
import { cpuUsageBetween, parseNetstatEthernetStatistics } from './windows'

describe('Windows system stats', () => {
  it('calculates CPU utilization from cumulative processor times', () => {
    const usage = cpuUsageBetween(
      { user: 100, nice: 0, system: 50, idle: 850, irq: 0 },
      { user: 150, nice: 0, system: 100, idle: 950, irq: 0 }
    )

    expect(usage).toEqual({
      usagePercent: 50,
      userPercent: 25,
      systemPercent: 25,
      idlePercent: 50,
    })
  })

  it('parses netstat byte counters without depending on the Windows display language', () => {
    expect(
      parseNetstatEthernetStatistics(`
Interface Statistics

                           Received            Sent
Bytes                      12345               67890
`)
    ).toEqual({ receivedBytes: 12345, sentBytes: 67890 })

    expect(
      parseNetstatEthernetStatistics(`
Статистика интерфейса

                           Получено            Отправлено
Байты                      900                 1200
`)
    ).toEqual({ receivedBytes: 900, sentBytes: 1200 })
  })
})
