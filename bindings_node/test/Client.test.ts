import { v4 } from 'uuid'
import { toBytes } from 'viem'
import { describe, expect, it } from 'vitest'
import { createClient, createRegisteredClient, createUser } from '@test/helpers'
import {
  NapiConsentEntityType,
  NapiConsentState,
  NapiSignatureRequestType,
} from '../dist'

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
    expect(inboxState.installations.length).toBe(1)
    expect(inboxState.installations[0].id).toBe(client.installationId())
    expect(inboxState.accountAddresses).toEqual([
      user.account.address.toLowerCase(),
    ])
    expect(inboxState.recoveryAddress).toBe(user.account.address.toLowerCase())

    const user2 = createUser()
    const client2 = await createClient(user2)
    const inboxState2 = await client2.getLatestInboxState(client.inboxId())
    expect(inboxState2.inboxId).toBe(client.inboxId())
    expect(inboxState.installations.length).toBe(1)
    expect(inboxState.installations[0].id).toBe(client.installationId())
    expect(inboxState2.accountAddresses).toEqual([
      user.account.address.toLowerCase(),
    ])
    expect(inboxState2.recoveryAddress).toBe(user.account.address.toLowerCase())
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
    expect(inboxState.installations.length).toBe(3)

    const installationIds = inboxState.installations.map((i) => i.id)
    expect(installationIds).toContain(client.installationId())
    expect(installationIds).toContain(client2.installationId())
    expect(installationIds).toContain(client3.installationId())

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

    expect(inboxState2.installations.length).toBe(1)
    expect(inboxState2.installations[0].id).toBe(client3.installationId())
  })

  it('should manage consent states', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1
      .conversations()
      .createGroup([user2.account.address])

    await client2.conversations().sync()
    const group2 = client2.conversations().findGroupById(group.id())

    expect(
      await client2.getConsentState(NapiConsentEntityType.GroupId, group2.id())
    ).toBe(NapiConsentState.Unknown)

    await client2.setConsentStates([
      {
        entityType: NapiConsentEntityType.GroupId,
        entity: group2.id(),
        state: NapiConsentState.Allowed,
      },
    ])

    expect(
      await client2.getConsentState(NapiConsentEntityType.GroupId, group2.id())
    ).toBe(NapiConsentState.Allowed)

    expect(group2.consentState()).toBe(NapiConsentState.Allowed)

    group2.updateConsentState(NapiConsentState.Denied)

    expect(
      await client2.getConsentState(NapiConsentEntityType.GroupId, group2.id())
    ).toBe(NapiConsentState.Denied)
  })
})
