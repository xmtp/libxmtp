import { describe, expect, it } from 'vitest'
import { createRegisteredClient, createUser } from '@test/helpers'
import {
  Actions,
  ActionsCodec,
  ActionStyle,
  Attachment,
  AttachmentCodec,
  GroupUpdatedCodec,
  IdentifierKind,
  Intent,
  IntentCodec,
  LeaveRequestCodec,
  MultiRemoteAttachment,
  MultiRemoteAttachmentCodec,
  Reaction,
  ReactionAction,
  ReactionCodec,
  ReactionSchema,
  ReadReceiptCodec,
  RemoteAttachment,
  RemoteAttachmentCodec,
  Reply,
  ReplyCodec,
  TextCodec,
  TransactionReference,
  TransactionReferenceCodec,
  WalletSendCalls,
  WalletSendCallsCodec,
} from '../dist'

describe.concurrent('Codecs', () => {
  it('should encode and decode text', () => {
    const contentType = TextCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('text')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const text = 'Hello, world!'
    const encoded = TextCodec.encode(text)
    expect(encoded.fallback).toEqual(undefined)
    const decoded = TextCodec.decode(encoded)
    expect(decoded).toEqual(text)
    expect(TextCodec.shouldPush()).toBe(true)
  })

  it('should encode and decode reactions', () => {
    const contentType = ReactionCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('reaction')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const reaction = {
      reference: '123',
      referenceInboxId: '456',
      action: ReactionAction.Added,
      content: '👍',
      schema: ReactionSchema.Unicode,
    } satisfies Reaction
    const encoded = ReactionCodec.encode(reaction)
    expect(encoded.fallback).toEqual(`Reacted with "👍" to an earlier message`)
    const decoded = ReactionCodec.decode(encoded)
    expect(decoded).toEqual(reaction)
    expect(ReactionCodec.shouldPush()).toBe(false)

    const reaction2 = {
      reference: '123',
      referenceInboxId: '456',
      action: ReactionAction.Removed,
      content: '👍',
      schema: ReactionSchema.Unicode,
    } satisfies Reaction
    const encoded2 = ReactionCodec.encode(reaction2)
    expect(encoded2.fallback).toEqual(`Removed "👍" from an earlier message`)
  })

  it('should encode and decode replies', () => {
    const contentType = ReplyCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('reply')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const reply = {
      reference: '123',
      referenceInboxId: '456',
      content: TextCodec.encode('Hello, world!'),
    } satisfies Reply
    const encoded = ReplyCodec.encode(reply)
    expect(encoded.fallback).toEqual(
      `Replied with "Hello, world!" to an earlier message`
    )
    const decoded = ReplyCodec.decode(encoded)
    expect(decoded).toEqual(reply)
    expect(ReplyCodec.shouldPush()).toBe(true)

    const reply2 = {
      reference: '123',
      referenceInboxId: '456',
      content: AttachmentCodec.encode({
        filename: 'attachment.png',
        mimeType: 'image/png',
        content: new Uint8Array(),
      }),
    } satisfies Reply
    const encoded2 = ReplyCodec.encode(reply2)
    expect(encoded2.fallback).toEqual(`Replied to an earlier message`)
  })

  it('should encode and decode read receipts', () => {
    const contentType = ReadReceiptCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('readReceipt')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const readReceipt = {}
    const encoded = ReadReceiptCodec.encode(readReceipt)
    expect(encoded.fallback).toEqual(undefined)
    const decoded = ReadReceiptCodec.decode(encoded)
    expect(decoded).toEqual(readReceipt)
    expect(ReadReceiptCodec.shouldPush()).toBe(false)
  })

  it('should encode and decode remote attachments', () => {
    const contentType = RemoteAttachmentCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('remoteStaticAttachment')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const remoteAttachment = {
      url: 'https://example.com/attachment.png',
      contentDigest: '123',
      secret: new Uint8Array(),
      salt: new Uint8Array(),
      nonce: new Uint8Array(),
      scheme: 'https',
      contentLength: 100,
      filename: 'attachment.png',
    } satisfies RemoteAttachment
    const encoded = RemoteAttachmentCodec.encode(remoteAttachment)
    expect(encoded.fallback).toEqual(
      `Can't display ${remoteAttachment.filename}. This app doesn't support remote attachments.`
    )
    const decoded = RemoteAttachmentCodec.decode(encoded)
    expect(decoded).toEqual(remoteAttachment)
    expect(RemoteAttachmentCodec.shouldPush()).toBe(true)
  })

  it('should encode and decode transaction reference', () => {
    const contentType = TransactionReferenceCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('transactionReference')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const transactionReference = {
      namespace: 'eip155',
      networkId: '1',
      reference: '123',
    } satisfies TransactionReference
    const encoded = TransactionReferenceCodec.encode(transactionReference)
    expect(encoded.fallback).toEqual(
      `[Crypto transaction] Use a blockchain explorer to learn more using the transaction hash: ${transactionReference.reference}`
    )
    const decoded = TransactionReferenceCodec.decode(encoded)
    expect(decoded).toEqual(transactionReference)
    expect(TransactionReferenceCodec.shouldPush()).toBe(true)

    const transactionReference2 = {
      namespace: 'eip155',
      networkId: '1',
      reference: '',
    } satisfies TransactionReference
    const encoded2 = TransactionReferenceCodec.encode(transactionReference2)
    expect(encoded2.fallback).toEqual(`Crypto transaction`)
  })

  it('should encode and decode wallet send calls', () => {
    const contentType = WalletSendCallsCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('walletSendCalls')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const walletSendCall = {
      version: '1',
      chainId: '1',
      from: '0x123',
      calls: [
        {
          to: '0x123',
          data: '0x123',
          value: '0x0',
          gas: '0x5208',
          metadata: {
            description: 'Send funds',
            transactionType: 'transfer',
            extra: {
              note: 'test',
            },
          },
        },
      ],
      capabilities: {
        foo: 'bar',
      },
    } satisfies WalletSendCalls
    const encoded = WalletSendCallsCodec.encode(walletSendCall)
    const decoded = WalletSendCallsCodec.decode(encoded)
    expect(decoded).toEqual(walletSendCall)
    expect(WalletSendCallsCodec.shouldPush()).toBe(true)
  })

  it('should encode and decode actions', () => {
    const contentType = ActionsCodec.contentType()
    expect(contentType.authorityId).toEqual('coinbase.com')
    expect(contentType.typeId).toEqual('actions')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const actions = {
      id: '1',
      description: 'Test actions',
      actions: [
        {
          id: '1',
          label: 'Test action',
          imageUrl: 'https://example.com/image.png',
          style: ActionStyle.Primary,
          expiresAtNs: 1700000000000000000,
        },
        {
          id: '2',
          label: 'Test action 2',
          imageUrl: 'https://example.com/image.png',
          style: ActionStyle.Secondary,
          expiresAtNs: 1700000000000000000,
        },
      ],
      expiresAtNs: 1700000000000000000,
    } satisfies Actions
    const encoded = ActionsCodec.encode(actions)
    expect(encoded.fallback).toEqual(
      `Test actions\n\n[1] Test action\n[2] Test action 2\n\nReply with the number to select`
    )
    const decoded = ActionsCodec.decode(encoded)
    expect(decoded).toEqual(actions)
    expect(ActionsCodec.shouldPush()).toBe(true)
  })

  it('should encode and decode attachments', () => {
    const contentType = AttachmentCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('attachment')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const attachment = {
      filename: 'attachment.png',
      mimeType: 'image/png',
      content: new Uint8Array(),
    } satisfies Attachment
    const encoded = AttachmentCodec.encode(attachment)
    expect(encoded.fallback).toEqual(
      `Can't display ${attachment.filename}. This app doesn't support attachments.`
    )
    const decoded = AttachmentCodec.decode(encoded)
    expect(decoded).toEqual(attachment)
    expect(AttachmentCodec.shouldPush()).toBe(true)
  })

  it('should encode and decode multi remote attachments', () => {
    const contentType = MultiRemoteAttachmentCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('multiRemoteStaticAttachment')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const multiRemoteAttachment = {
      attachments: [
        {
          url: 'https://example.com/attachment.png',
          contentDigest: '123',
          secret: new Uint8Array(),
          salt: new Uint8Array(),
          nonce: new Uint8Array(),
          scheme: 'https',
          contentLength: 100,
          filename: 'attachment.png',
        },
        {
          url: 'https://example.com/attachment.pdf',
          contentDigest: '456',
          secret: new Uint8Array(),
          salt: new Uint8Array(),
          nonce: new Uint8Array(),
          scheme: 'https',
          contentLength: 200,
          filename: 'attachment.pdf',
        },
      ],
    } satisfies MultiRemoteAttachment
    const encoded = MultiRemoteAttachmentCodec.encode(multiRemoteAttachment)
    expect(encoded.fallback).toEqual(
      "Can't display this content. This app doesn't support multiple remote attachments."
    )
    const decoded = MultiRemoteAttachmentCodec.decode(encoded)
    expect(decoded).toEqual(multiRemoteAttachment)
    expect(MultiRemoteAttachmentCodec.shouldPush()).toBe(true)
  })

  it('should encode and decode intents', () => {
    const contentType = IntentCodec.contentType()
    expect(contentType.authorityId).toEqual('coinbase.com')
    expect(contentType.typeId).toEqual('intent')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)
    const intent = {
      id: '1',
      actionId: '1',
      metadata: {
        foo: 'bar',
      },
    } satisfies Intent
    const encoded = IntentCodec.encode(intent)
    expect(encoded.fallback).toEqual(`User selected action: ${intent.actionId}`)
    const decoded = IntentCodec.decode(encoded)
    expect(decoded).toEqual(intent)
    expect(IntentCodec.shouldPush()).toBe(true)
  })

  it('should decode group updated', async () => {
    const contentType = GroupUpdatedCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('group_updated')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)

    const user = createUser()
    const client1 = await createRegisteredClient(user)

    const user2 = createUser()
    const client2 = await createRegisteredClient(user2)

    const group = await client1
      .conversations()
      .createGroupByInboxId([client2.inboxId()])

    const messages = await group.findMessages()
    const groupUpdated = GroupUpdatedCodec.decode(messages[0].content)
    expect(groupUpdated).toEqual({
      initiatedByInboxId: client1.inboxId(),
      addedInboxes: [
        {
          inboxId: client2.inboxId(),
        },
      ],
      removedInboxes: [],
      metadataFieldChanges: [],
      leftInboxes: [],
      addedAdminInboxes: [],
      addedSuperAdminInboxes: [],
      removedAdminInboxes: [],
      removedSuperAdminInboxes: [],
    })
  })

  it('should decode leave request', async () => {
    const contentType = LeaveRequestCodec.contentType()
    expect(contentType.authorityId).toEqual('xmtp.org')
    expect(contentType.typeId).toEqual('leave_request')
    expect(contentType.versionMajor).toBeGreaterThan(0)
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0)

    const user = createUser()
    const client1 = await createRegisteredClient(user)

    const user2 = createUser()
    const client2 = await createRegisteredClient(user2)

    const group = await client1
      .conversations()
      .createGroupByInboxId([client2.inboxId()])

    await client2.conversations().sync()
    const group2 = client2.conversations().findGroupById(group.id())
    await group2.leaveGroup()

    const messages = await group2.findMessages()
    const leaveRequest = LeaveRequestCodec.decode(messages[1].content)
    expect(leaveRequest).toEqual({})
  })
})
