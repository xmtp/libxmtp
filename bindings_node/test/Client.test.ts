import { describe, expect, it } from 'vitest'
import { createClient, createRegisteredClient, createUser } from '@test/helpers'

describe('Client', () => {
  it('should not be registered at first', async () => {
    const user = createUser()
    const client = await createClient(user)
    expect(client.isRegistered()).toBe(false)
  })

  it('should be registered aafter registration', async () => {
    const user = createUser()
    // must create 2 clients to get the expected value
    // this is currently a limitation in the rust implementation as the
    // underlying signature request does not mutate after registration
    await createRegisteredClient(user)
    const client = await createClient(user)
    expect(client.isRegistered()).toBe(true)
  })

  it('should be able to message registered identity', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const canMessage = await client.canMessage([user.account.address])
    expect(canMessage).toEqual({ [user.account.address.toLowerCase()]: true })
  })
})
