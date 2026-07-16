import { describe, expect, it } from 'vitest'
import { createRegisteredClient, createUser } from '@test/helpers'
import { IdentifierKind } from '../dist'

describe('catchUpToLive', () => {
  it('cold-catches a pending group and its history, then is idempotent', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)

    // client1 creates a group with client2 and sends. client2 has never
    // synced or streamed, so both the welcome and the message are owed.
    const group = await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await group.sendText('missed while away')

    expect(client2.conversations().list().length).toBe(0)

    const summary = await client2.catchUpToLive()
    expect(Number(summary.conversations)).toBeGreaterThanOrEqual(1)
    expect(Number(summary.messages)).toBeGreaterThanOrEqual(1)
    expect(client2.conversations().list().length).toBe(1)

    // Nothing owed now: a second run persists nothing new.
    const again = await client2.catchUpToLive()
    expect(Number(again.messages)).toBe(0)
  })
})
