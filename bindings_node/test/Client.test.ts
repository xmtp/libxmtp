import { v4 } from 'uuid'
import { toBytes } from 'viem'
import { describe, expect, it } from 'vitest'
import { createClient, createRegisteredClient, createUser } from '@test/helpers'
import { NapiSignatureRequestType } from '../dist'

describe('Client', () => {
  it('should not be registered at first', async () => {
    const user = createUser()
    const client = await createClient(user)
    expect(client.isRegistered()).toBe(false)
  })

  it('should be registered after registration', async () => {
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

  it('should find an inbox ID from an address', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const inboxId = await client.findInboxIdByAddress(user.account.address)
    expect(inboxId).toBe(client.inboxId())
  })

  it('should return the correct inbox state', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const inboxState = await client.inboxState(false)
    expect(inboxState.inboxId).toBe(client.inboxId())
    expect(inboxState.installationIds).toEqual([client.installationId()])
    expect(inboxState.accountAddresses).toEqual([
      user.account.address.toLowerCase(),
    ])
    expect(inboxState.recoveryAddress).toBe(user.account.address.toLowerCase())
  })

  it('should add a wallet association to the client', async () => {
    const user = createUser()
    const user2 = createUser()
    const client = await createRegisteredClient(user)
    const signatureText = await client.addWalletSignatureText(
      user.account.address,
      user2.account.address
    )
    expect(signatureText).toBeDefined()

    // sign message
    const signature = await user.wallet.signMessage({
      message: signatureText,
    })
    const signature2 = await user2.wallet.signMessage({
      message: signatureText,
    })

    await client.addSignature(
      NapiSignatureRequestType.AddWallet,
      toBytes(signature)
    )
    await client.addSignature(
      NapiSignatureRequestType.AddWallet,
      toBytes(signature2)
    )
    await client.applySignatureRequests()
    const inboxState = await client.inboxState(false)
    expect(inboxState.accountAddresses.length).toEqual(2)
    expect(inboxState.accountAddresses).toContain(
      user.account.address.toLowerCase()
    )
    expect(inboxState.accountAddresses).toContain(
      user2.account.address.toLowerCase()
    )
  })

  it('should revoke a wallet association from the client', async () => {
    const user = createUser()
    const user2 = createUser()
    const client = await createRegisteredClient(user)
    const signatureText = await client.addWalletSignatureText(
      user.account.address,
      user2.account.address
    )
    expect(signatureText).toBeDefined()

    // sign message
    const signature = await user.wallet.signMessage({
      message: signatureText,
    })
    const signature2 = await user2.wallet.signMessage({
      message: signatureText,
    })

    await client.addSignature(
      NapiSignatureRequestType.AddWallet,
      toBytes(signature)
    )
    await client.addSignature(
      NapiSignatureRequestType.AddWallet,
      toBytes(signature2)
    )
    await client.applySignatureRequests()

    const signatureText2 = await client.revokeWalletSignatureText(
      user2.account.address
    )
    expect(signatureText2).toBeDefined()

    // sign message
    const signature3 = await user.wallet.signMessage({
      message: signatureText2,
    })

    await client.addSignature(
      NapiSignatureRequestType.RevokeWallet,
      toBytes(signature3)
    )
    await client.applySignatureRequests()
    const inboxState = await client.inboxState(false)
    expect(inboxState.accountAddresses).toEqual([
      user.account.address.toLowerCase(),
    ])
  })

  it('should revoke all installations', async () => {
    const user = createUser()

    const client = await createRegisteredClient(user)
    user.uuid = v4()
    const client2 = await createRegisteredClient(user)
    user.uuid = v4()
    const client3 = await createRegisteredClient(user)

    const inboxState = await client3.inboxState(true)
    expect(inboxState.installationIds.length).toEqual(3)
    expect(inboxState.installationIds).toContain(client.installationId())
    expect(inboxState.installationIds).toContain(client2.installationId())
    expect(inboxState.installationIds).toContain(client3.installationId())

    const signatureText = await client3.revokeInstallationsSignatureText()
    expect(signatureText).toBeDefined()

    // sign message
    const signature = await user.wallet.signMessage({
      message: signatureText,
    })

    await client3.addSignature(
      NapiSignatureRequestType.RevokeInstallations,
      toBytes(signature)
    )
    await client3.applySignatureRequests()
    const inboxState2 = await client3.inboxState(true)
    expect(inboxState2.installationIds).toEqual([client3.installationId()])
  })
})
