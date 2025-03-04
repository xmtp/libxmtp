import { v4 } from 'uuid'
import { toBytes } from 'viem'
import { describe, expect, it } from 'vitest'
import {
  createClient,
  createRegisteredClient,
  createUser,
  encodeTextMessage,
  sleep,
} from '@test/helpers'
import {
  ConsentEntityType,
  ConsentState,
  PublicIdentifierKind,
  SignatureRequestType,
  verifySignedWithPublicKey,
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
    const canMessage = await client.canMessage([
      {
        identifier: user.account.address,
        identifierKind: PublicIdentifierKind.Ethereum,
      },
    ])
    expect(canMessage).toEqual([true])
  })

  it('should find an inbox ID from an address', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const inboxId = await client.findInboxIdByIdentifier({
      identifier: user.account.address,
      identifierKind: PublicIdentifierKind.Ethereum,
    })
    expect(inboxId).toBe(client.inboxId())
  })

  it('should return the correct inbox state', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const inboxState = await client.inboxState(false)
    expect(inboxState.inboxId).toBe(client.inboxId())
    expect(inboxState.installations.length).toBe(1)
    expect(inboxState.installations[0].id).toBe(client.installationId())
    expect(inboxState.installations[0].bytes).toEqual(
      client.installationIdBytes()
    )
    expect(inboxState.identifiers).toEqual([
      {
        identifier: user.account.address.toLowerCase(),
        identifierKind: PublicIdentifierKind.Ethereum,
      },
    ])
    expect(inboxState.recoveryIdentifier).toStrictEqual({
      identifier: user.account.address.toLowerCase(),
      identifierKind: PublicIdentifierKind.Ethereum,
    })

    const user2 = createUser()
    const client2 = await createClient(user2)
    const inboxState2 = await client2.getLatestInboxState(client.inboxId())
    expect(inboxState2.inboxId).toBe(client.inboxId())
    expect(inboxState.installations.length).toBe(1)
    expect(inboxState.installations[0].id).toBe(client.installationId())
    expect(inboxState.installations[0].bytes).toEqual(
      client.installationIdBytes()
    )
    expect(inboxState2.identifiers).toEqual([
      {
        identifier: user.account.address.toLowerCase(),
        identifierKind: PublicIdentifierKind.Ethereum,
      },
    ])
    expect(inboxState2.recoveryIdentifier).toEqual({
      identifier: user.account.address.toLowerCase(),
      identifierKind: PublicIdentifierKind.Ethereum,
    })
  })

  it('should add a wallet association to the client', async () => {
    const user = createUser()
    const user2 = createUser()
    const client = await createRegisteredClient(user)
    const signatureText = await client.addIdentifierSignatureText({
      identifier: user2.account.address,
      identifierKind: PublicIdentifierKind.Ethereum,
    })
    expect(signatureText).toBeDefined()

    const signature2 = await user2.wallet.signMessage({
      message: signatureText,
    })

    await client.addSignature(
      SignatureRequestType.AddWallet,
      toBytes(signature2)
    )
    await client.applySignatureRequests()
    const inboxState = await client.inboxState(false)
    expect(inboxState.identifiers.length).toEqual(2)
    expect(inboxState.identifiers).toContainEqual({
      identifier: user.account.address.toLowerCase(),
      identifierKind: PublicIdentifierKind.Ethereum,
    })
    expect(inboxState.identifiers).toContainEqual({
      identifier: user2.account.address.toLowerCase(),
      identifierKind: PublicIdentifierKind.Ethereum,
    })
  })

  it('should revoke a wallet association from the client', async () => {
    const user = createUser()
    const user2 = createUser()
    const client = await createRegisteredClient(user)
    const signatureText = await client.addIdentifierSignatureText({
      identifier: user2.account.address,
      identifierKind: PublicIdentifierKind.Ethereum,
    })
    expect(signatureText).toBeDefined()

    // sign message
    const signature2 = await user2.wallet.signMessage({
      message: signatureText,
    })

    await client.addSignature(
      SignatureRequestType.AddWallet,
      toBytes(signature2)
    )
    await client.applySignatureRequests()

    const signatureText2 = await client.revokeIdentifierSignatureText({
      identifier: user2.account.address,
      identifierKind: PublicIdentifierKind.Ethereum,
    })
    expect(signatureText2).toBeDefined()

    // sign message
    const signature3 = await user.wallet.signMessage({
      message: signatureText2,
    })

    await client.addSignature(
      SignatureRequestType.RevokeWallet,
      toBytes(signature3)
    )
    await client.applySignatureRequests()
    const inboxState = await client.inboxState(false)
    expect(inboxState.identifiers).toEqual([
      {
        identifier: user.account.address.toLowerCase(),
        identifierKind: PublicIdentifierKind.Ethereum,
      },
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

    const signatureText =
      await client3.revokeAllOtherInstallationsSignatureText()
    expect(signatureText).toBeDefined()

    // sign message
    const signature = await user.wallet.signMessage({
      message: signatureText,
    })

    await client3.addSignature(
      SignatureRequestType.RevokeInstallations,
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
    const group = await client1.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: PublicIdentifierKind.Ethereum,
      },
    ])

    await client2.conversations().sync()
    const group2 = client2.conversations().findGroupById(group.id())

    expect(
      await client2.getConsentState(ConsentEntityType.GroupId, group2.id())
    ).toBe(ConsentState.Unknown)

    await client2.setConsentStates([
      {
        entityType: ConsentEntityType.GroupId,
        entity: group2.id(),
        state: ConsentState.Allowed,
      },
    ])

    expect(
      await client2.getConsentState(ConsentEntityType.GroupId, group2.id())
    ).toBe(ConsentState.Allowed)

    expect(group2.consentState()).toBe(ConsentState.Allowed)

    group2.updateConsentState(ConsentState.Denied)

    expect(
      await client2.getConsentState(ConsentEntityType.GroupId, group2.id())
    ).toBe(ConsentState.Denied)
  })

  it('should get inbox addresses', async () => {
    const user = createUser()
    const user2 = createUser()
    const client = await createRegisteredClient(user)
    const client2 = await createRegisteredClient(user2)
    const inboxAddresses = await client.addressesFromInboxId(true, [
      client.inboxId(),
    ])
    expect(inboxAddresses.length).toBe(1)
    expect(inboxAddresses[0].inboxId).toBe(client.inboxId())
    expect(inboxAddresses[0].identifiers).toEqual([
      {
        identifier: user.account.address.toLowerCase(),
        identifierKind: PublicIdentifierKind.Ethereum,
      },
    ])

    const inboxAddresses2 = await client2.addressesFromInboxId(true, [
      client2.inboxId(),
    ])
    expect(inboxAddresses2.length).toBe(1)
    expect(inboxAddresses2[0].inboxId).toBe(client2.inboxId())
    expect(inboxAddresses2[0].identifiers).toEqual([
      {
        identifier: user2.account.address.toLowerCase(),
        identifierKind: PublicIdentifierKind.Ethereum,
      },
    ])
  })

  it('should sign and verify with installation key', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const text = 'gm!'
    const signature = client.signWithInstallationKey(text)
    expect(signature).toBeDefined()
    expect(() =>
      client.verifySignedWithInstallationKey(text, signature)
    ).not.toThrow()
    expect(() =>
      client.verifySignedWithInstallationKey(text, new Uint8Array())
    ).toThrow()
    expect(() =>
      verifySignedWithPublicKey(text, signature, client.installationIdBytes())
    ).not.toThrow()
    expect(() =>
      verifySignedWithPublicKey(text, signature, new Uint8Array())
    ).toThrow()
  })
})

describe('Streams', () => {
  it('should stream all messages', async () => {
    const user = createUser()
    const client1 = await createRegisteredClient(user)

    const user2 = createUser()
    const client2 = await createRegisteredClient(user2)

    const group = await client1.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: PublicIdentifierKind.Ethereum,
      },
    ])

    await client2.conversations().sync()
    const group2 = client2.conversations().findGroupById(group.id())

    let messages = new Array()
    client2.conversations().syncAllConversations()
    let stream = client2.conversations().streamAllMessages((msg) => {
      messages.push(msg)
    })
    await stream.waitForReady()
    group.send(encodeTextMessage('Test1'))
    group.send(encodeTextMessage('Test2'))
    group.send(encodeTextMessage('Test3'))
    group.send(encodeTextMessage('Test4'))
    await sleep(1000)
    await stream.endAndWait()
    expect(messages.length).toBe(4)
  })
})
