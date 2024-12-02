import { describe, expect, it } from 'vitest'
import { createRegisteredClient, createUser, TEST_API_URL } from '@test/helpers'
import {
  generateInboxId,
  getInboxIdForAddress,
  isAddressAuthorized,
  isInstallationAuthorized,
} from '../dist/index'

describe('generateInboxId', () => {
  it('should generate an inbox id', () => {
    const user = createUser()
    const inboxId = generateInboxId(user.account.address)
    expect(inboxId).toBeDefined()
  })
})

describe('getInboxIdForAddress', () => {
  it('should return `null` inbox ID for unregistered address', async () => {
    const user = createUser()
    const inboxId = await getInboxIdForAddress(
      TEST_API_URL,
      false,
      user.account.address
    )
    expect(inboxId).toBe(null)
  })

  it('should return inbox ID for registered address', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const inboxId = await getInboxIdForAddress(
      TEST_API_URL,
      false,
      user.account.address
    )
    expect(inboxId).toBe(client.inboxId())
  })
})

describe('isInstallationAuthorized', () => {
  it('should return true if installation is authorized', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const isAuthorized = await isInstallationAuthorized(
      TEST_API_URL,
      client.inboxId(),
      client.installationIdBytes()
    )
    expect(isAuthorized).toBe(true)
  })
})

describe('isAddressAuthorized', () => {
  it('should return true if address is authorized', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const isAuthorized = await isAddressAuthorized(
      TEST_API_URL,
      client.inboxId(),
      user.account.address
    )
    expect(isAuthorized).toBe(true)
  })
})
