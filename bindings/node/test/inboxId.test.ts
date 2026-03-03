import { describe, expect, it } from 'vitest'
import {
  createLocalBackend,
  createRegisteredClient,
  createUser,
} from '@test/helpers'
import {
  generateInboxId,
  getInboxIdByIdentity,
  IdentifierKind,
  isAddressAuthorized,
  isInstallationAuthorized,
} from '../dist/index'

describe('generateInboxId', () => {
  it('should generate an inbox id', () => {
    const user = createUser()
    const inboxId = generateInboxId({
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
    expect(inboxId).toBeDefined()
  })

  it('should throw error with [ErrorType::Variant] format for invalid address', () => {
    expect(() =>
      generateInboxId({
        identifier: 'invalid-address',
        identifierKind: IdentifierKind.Ethereum,
      })
    ).toThrow(/^\[IdentifierValidationError::InvalidAddresses\].*/)
  })
})

describe('getInboxIdByIdentity', () => {
  it('should return `null` inbox ID for unregistered address', async () => {
    const user = createUser()
    const backend = await createLocalBackend()
    const inboxId = await getInboxIdByIdentity(backend, {
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
    expect(inboxId).toBe(null)
  })

  it('should return inbox ID for registered address', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const backend = await createLocalBackend()
    const inboxId = await getInboxIdByIdentity(backend, {
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
    expect(inboxId).toBe(client.inboxId())
  })
})

describe('isInstallationAuthorized', () => {
  it('should return true if installation is authorized', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const backend = await createLocalBackend()
    const isAuthorized = await isInstallationAuthorized(
      backend,
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
    const backend = await createLocalBackend()
    const isAuthorized = await isAddressAuthorized(
      backend,
      client.inboxId(),
      user.account.address
    )
    expect(isAuthorized).toBe(true)
  })
})
