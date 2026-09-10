import { describe, expect, it } from 'vitest'
import {
  BackendBuilder,
  createClientWithBackend,
  generateInboxId,
  IdentifierKind,
  SyncWorkerMode,
  type StreamSettings,
} from '../dist'

const invalidClient = async (streamSettings: StreamSettings) => {
  // Validation occurs before a network request. This address has no backend.
  const backend = await new BackendBuilder('http://127.0.0.1:1').build()
  const identifier = {
    identifier: '0x0000000000000000000000000000000000000001',
    identifierKind: IdentifierKind.Ethereum,
  }
  return createClientWithBackend(
    backend,
    {},
    generateInboxId(identifier),
    identifier,
    SyncWorkerMode.Disabled,
    undefined,
    undefined,
    true,
    undefined,
    undefined,
    streamSettings
  )
}

describe('stream settings validation', () => {
  it.each([-1, 0, 1.5, Number.NaN, Number.POSITIVE_INFINITY, 2 ** 32])(
    'rejects an invalid row limit before unsigned conversion: %s',
    async (maxLocalReadRows) => {
      await expect(invalidClient({ maxLocalReadRows })).rejects.toThrow(
        'invalid stream setting'
      )
    }
  )

  it.each([-1n, 0n, 1n << 64n])(
    'rejects an invalid byte budget: %s',
    async (maxLocalReadBytes) => {
      await expect(invalidClient({ maxLocalReadBytes })).rejects.toThrow(
        'invalid stream setting'
      )
    }
  )

  it('uses core timer relationships', async () => {
    await expect(
      invalidClient({
        activeDatabasePollIntervalMs: 100,
        defaultConsumerLeaseDurationMs: 100,
      })
    ).rejects.toThrow('invalid stream setting')
  })
})
