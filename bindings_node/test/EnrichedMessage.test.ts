import { describe, expect, it } from 'vitest'
import { createRegisteredClient, createUser } from '@test/helpers'
import {
  Actions,
  ActionStyle,
  Attachment,
  DecodedMessageContentType,
  encodeAttachment,
  encodeText,
  IdentifierKind,
  Intent,
  MultiRemoteAttachment,
  ReactionAction,
  ReactionSchema,
  RemoteAttachment,
  SortDirection,
  TransactionReference,
  WalletSendCalls,
} from '../dist'

describe.concurrent('EnrichedMessage', () => {
  // Helper to set up a basic group conversation
  const setupConversation = async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const conversation = await client1.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client2.conversations().sync()
    const conversation2 = client2
      .conversations()
      .findGroupById(conversation.id())
    return { user1, user2, client1, client2, conversation, conversation2 }
  }

  describe('Basic message retrieval', () => {
    it('should return enriched messages with basic fields populated', async () => {
      const { client1, conversation } = await setupConversation()

      await conversation.sendText('Hello World')
      await conversation.sendText('Second message')

      const messages = await conversation.findEnrichedMessages()
      expect(messages.length).toEqual(3)

      const textMessages = messages.filter((m) => m.content.text !== null)
      expect(textMessages.length).toEqual(2)

      const helloWorldMessage = textMessages.find(
        (m) => m.content.text === 'Hello World'
      )
      expect(helloWorldMessage).toBeDefined()
      expect(helloWorldMessage!.id).toBeDefined()
      expect(helloWorldMessage!.sentAtNs).toBeDefined()
      expect(helloWorldMessage!.senderInboxId).toBe(client1.inboxId())
      expect(helloWorldMessage!.conversationId).toBeDefined()
      expect(helloWorldMessage!.content.text).toBeDefined()
      expect(helloWorldMessage!.content.text).toBe('Hello World')
      expect(helloWorldMessage!.deliveryStatus).toBeDefined()
    })

    it('should handle list options', async () => {
      const { conversation } = await setupConversation()

      await conversation.sendText('Message 1')
      await conversation.sendText('Message 2')
      await conversation.sendText('Message 3')

      const limitedMessages = await conversation.findEnrichedMessages({
        limit: 2,
        direction: SortDirection.Descending,
      })
      const limitedTextMessages = limitedMessages.filter(
        (m) => m.content.text !== null
      )
      expect(limitedTextMessages.length).toBe(2)

      const allMessages = await conversation.findEnrichedMessages()
      const allTextMessages = allMessages.filter((m) => m.content.text !== null)
      expect(allTextMessages.length).toEqual(3)
    })
  })

  describe('Message metadata', () => {
    it('should include message kind', async () => {
      const { conversation } = await setupConversation()

      await conversation.sendText('Test')

      const messages = await conversation.findEnrichedMessages()

      expect(messages.length).toEqual(2)
      // Messages should have kinds defined
      const messagesWithKind = messages.filter((m) => m.kind !== undefined)
      expect(messagesWithKind.length).toEqual(2)
    })
  })

  describe('Content types', () => {
    describe('Text', () => {
      it('should send and receive text message', async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation()

        const messageId = await conversation.sendText('Hello, world!')
        expect(messageId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const textMessage = messages.find((m) => m.id === messageId)
        expect(textMessage).toBeDefined()
        expect(textMessage!.content.type).toBe(DecodedMessageContentType.Text)
        expect(textMessage!.content.text).toBe('Hello, world!')
        expect(textMessage!.senderInboxId).toBe(client1.inboxId())
        expect(textMessage!.contentType?.authorityId).toBe('xmtp.org')
        expect(textMessage!.contentType?.typeId).toBe('text')
        // Text has no fallback (napi-rs returns null for Option::None)
        expect(textMessage!.fallback).toBeNull()
      })
    })

    describe('Markdown', () => {
      it('should send and receive a markdown message', async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation()

        const messageId = await conversation.sendMarkdown('# Hello, world!')
        expect(messageId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const markdownMessage = messages.find((m) => m.id === messageId)
        expect(markdownMessage).toBeDefined()
        expect(markdownMessage!.content.type).toBe(
          DecodedMessageContentType.Markdown
        )
        expect(markdownMessage!.content.markdown).toBe('# Hello, world!')
        expect(markdownMessage!.senderInboxId).toBe(client1.inboxId())
        expect(markdownMessage!.contentType?.authorityId).toBe('xmtp.org')
        expect(markdownMessage!.contentType?.typeId).toBe('markdown')
        // Markdown has no fallback (napi-rs returns null for Option::None)
        expect(markdownMessage!.fallback).toBeNull()
      })
    })

    describe('Reaction', () => {
      it('should send and receive reaction with Added action', async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation()

        const textMessageId = await conversation.sendText('Hello!')

        const reactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId(),
          action: ReactionAction.Added,
          content: '👍',
          schema: ReactionSchema.Unicode,
        })
        expect(reactionId).toBeDefined()

        await conversation2.sync()

        // Reactions are attached to parent messages in enriched messages
        const messages = await conversation2.findEnrichedMessages()
        const textMessage = messages.find((m) => m.id === textMessageId)
        expect(textMessage).toBeDefined()
        expect(textMessage!.reactions.length).toBe(1)

        const reactionOnMessage = textMessage!.reactions[0]
        expect(reactionOnMessage.id).toBe(reactionId)
        expect(reactionOnMessage.content.type).toBe(
          DecodedMessageContentType.Reaction
        )
        expect(reactionOnMessage.content.reaction?.content).toBe('👍')
        expect(reactionOnMessage.content.reaction?.action).toBe(
          ReactionAction.Added
        )
        expect(reactionOnMessage.content.reaction?.schema).toBe(
          ReactionSchema.Unicode
        )
        expect(reactionOnMessage.senderInboxId).toBe(client1.inboxId())
        // Reaction Added fallback
        expect(reactionOnMessage.fallback).toBe(
          `Reacted with "👍" to an earlier message`
        )
      })

      it('should send and receive reaction with Removed action', async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation()

        const textMessageId = await conversation.sendText('Hello!')

        // First add a reaction
        await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId(),
          action: ReactionAction.Added,
          content: '👍',
          schema: ReactionSchema.Unicode,
        })

        // Then remove it
        const removeReactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId(),
          action: ReactionAction.Removed,
          content: '👍',
          schema: ReactionSchema.Unicode,
        })
        expect(removeReactionId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const textMessage = messages.find((m) => m.id === textMessageId)
        expect(textMessage).toBeDefined()
        // After removal, the reactions array should reflect the removal
        const removedReaction = textMessage!.reactions.find(
          (r) => r.content.reaction?.action === ReactionAction.Removed
        )
        expect(removedReaction).toBeDefined()
        expect(removedReaction!.content.reaction?.content).toBe('👍')
        // Reaction Removed fallback
        expect(removedReaction!.fallback).toBe(
          `Removed "👍" from an earlier message`
        )
      })

      it('should handle shortcode reaction schema', async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation()

        const textMessageId = await conversation.sendText('Hello!')

        const reactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId(),
          action: ReactionAction.Added,
          content: ':thumbsup:',
          schema: ReactionSchema.Shortcode,
        })

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const textMessage = messages.find((m) => m.id === textMessageId)
        const reaction = textMessage!.reactions.find((r) => r.id === reactionId)
        expect(reaction).toBeDefined()
        expect(reaction!.content.reaction?.content).toBe(':thumbsup:')
        expect(reaction!.content.reaction?.schema).toBe(
          ReactionSchema.Shortcode
        )
      })

      it('should handle custom reaction schema', async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation()

        const textMessageId = await conversation.sendText('Hello!')

        const reactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId(),
          action: ReactionAction.Added,
          content: 'custom-reaction-id',
          schema: ReactionSchema.Custom,
        })

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const textMessage = messages.find((m) => m.id === textMessageId)
        const reaction = textMessage!.reactions.find((r) => r.id === reactionId)
        expect(reaction).toBeDefined()
        expect(reaction!.content.reaction?.content).toBe('custom-reaction-id')
        expect(reaction!.content.reaction?.schema).toBe(ReactionSchema.Custom)
      })
    })

    describe('Reply', () => {
      it('should send and receive reply with text content', async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation()

        const textMessageId = await conversation.sendText('Original message')

        const replyId = await conversation.sendReply({
          reference: textMessageId,
          referenceInboxId: client1.inboxId(),
          content: encodeText('This is a reply'),
        })
        expect(replyId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const replyMessage = messages.find((m) => m.id === replyId)
        expect(replyMessage).toBeDefined()
        expect(replyMessage!.content.type).toBe(DecodedMessageContentType.Reply)
        expect(replyMessage!.content.reply?.referenceId).toBe(textMessageId)
        expect(replyMessage!.content.reply?.content.type).toBe(
          DecodedMessageContentType.Text
        )
        expect(replyMessage!.content.reply?.content.text).toBe(
          'This is a reply'
        )
        expect(replyMessage!.senderInboxId).toBe(client1.inboxId())
        // Reply with text content fallback
        expect(replyMessage!.fallback).toBe(
          `Replied with "This is a reply" to an earlier message`
        )
      })

      it('should include inReplyTo with original message', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const textMessageId = await conversation.sendText('Original message')

        const replyId = await conversation.sendReply({
          reference: textMessageId,
          content: encodeText('Reply to original'),
        })

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const replyMessage = messages.find((m) => m.id === replyId)
        expect(replyMessage).toBeDefined()
        expect(replyMessage!.content.reply?.inReplyTo).toBeDefined()
        expect(replyMessage!.content.reply?.inReplyTo?.content.text).toBe(
          'Original message'
        )
      })

      it('should send and receive reply with non-text content (attachment)', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const textMessageId = await conversation.sendText('Original message')

        const replyId = await conversation.sendReply({
          reference: textMessageId,
          content: encodeAttachment({
            filename: 'reply.png',
            mimeType: 'image/png',
            content: new Uint8Array([137, 80, 78, 71]),
          }),
        })

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const replyMessage = messages.find((m) => m.id === replyId)
        expect(replyMessage).toBeDefined()
        expect(replyMessage!.content.type).toBe(DecodedMessageContentType.Reply)
        expect(replyMessage!.content.reply?.content.type).toBe(
          DecodedMessageContentType.Attachment
        )
        // Reply with non-text content fallback (generic)
        expect(replyMessage!.fallback).toBe(`Replied to an earlier message`)
      })
    })

    describe('Attachment', () => {
      it('should send and receive attachment', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const attachment: Attachment = {
          filename: 'test.txt',
          mimeType: 'text/plain',
          content: new Uint8Array([72, 101, 108, 108, 111]), // "Hello"
        }

        const attachmentId = await conversation.sendAttachment(attachment)
        expect(attachmentId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const attachmentMessage = messages.find((m) => m.id === attachmentId)
        expect(attachmentMessage).toBeDefined()
        expect(attachmentMessage!.content.type).toBe(
          DecodedMessageContentType.Attachment
        )
        expect(attachmentMessage!.content.attachment?.filename).toBe('test.txt')
        expect(attachmentMessage!.content.attachment?.mimeType).toBe(
          'text/plain'
        )
        expect(attachmentMessage!.content.attachment?.content).toEqual(
          new Uint8Array([72, 101, 108, 108, 111])
        )
        expect(attachmentMessage!.contentType?.typeId).toBe('attachment')
        // Attachment fallback
        expect(attachmentMessage!.fallback).toBe(
          `Can't display test.txt. This app doesn't support attachments.`
        )
      })

      it('should send and receive attachment without filename', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const attachment: Attachment = {
          mimeType: 'image/png',
          content: new Uint8Array([137, 80, 78, 71]), // PNG header
        }

        const attachmentId = await conversation.sendAttachment(attachment)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const attachmentMessage = messages.find((m) => m.id === attachmentId)
        expect(attachmentMessage).toBeDefined()
        expect(attachmentMessage!.content.attachment?.filename).toBeUndefined()
        expect(attachmentMessage!.content.attachment?.mimeType).toBe(
          'image/png'
        )
      })
    })

    describe('Remote Attachment', () => {
      it('should send and receive remote attachment', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const remoteAttachment: RemoteAttachment = {
          url: 'https://example.com/file.png',
          contentDigest: 'abc123',
          secret: new Uint8Array([1, 2, 3]),
          salt: new Uint8Array([4, 5, 6]),
          nonce: new Uint8Array([7, 8, 9]),
          scheme: 'https',
          contentLength: 1000,
          filename: 'file.png',
        }

        const remoteAttachmentId =
          await conversation.sendRemoteAttachment(remoteAttachment)
        expect(remoteAttachmentId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const remoteAttachmentMessage = messages.find(
          (m) => m.id === remoteAttachmentId
        )
        expect(remoteAttachmentMessage).toBeDefined()
        expect(remoteAttachmentMessage!.content.type).toBe(
          DecodedMessageContentType.RemoteAttachment
        )
        expect(remoteAttachmentMessage!.content.remoteAttachment?.url).toBe(
          'https://example.com/file.png'
        )
        expect(
          remoteAttachmentMessage!.content.remoteAttachment?.filename
        ).toBe('file.png')
        expect(
          remoteAttachmentMessage!.content.remoteAttachment?.contentDigest
        ).toBe('abc123')
        expect(remoteAttachmentMessage!.content.remoteAttachment?.scheme).toBe(
          'https'
        )
        expect(
          remoteAttachmentMessage!.content.remoteAttachment?.contentLength
        ).toBe(1000)
        expect(remoteAttachmentMessage!.contentType?.typeId).toBe(
          'remoteStaticAttachment'
        )
        // Remote attachment fallback
        expect(remoteAttachmentMessage!.fallback).toBe(
          `Can't display file.png. This app doesn't support remote attachments.`
        )
      })

      it('should send and receive remote attachment without filename', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const remoteAttachment: RemoteAttachment = {
          url: 'https://example.com/file',
          contentDigest: 'xyz789',
          secret: new Uint8Array([10, 11, 12]),
          salt: new Uint8Array([13, 14, 15]),
          nonce: new Uint8Array([16, 17, 18]),
          scheme: 'https',
          contentLength: 500,
        }

        const remoteAttachmentId =
          await conversation.sendRemoteAttachment(remoteAttachment)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const remoteAttachmentMessage = messages.find(
          (m) => m.id === remoteAttachmentId
        )
        expect(remoteAttachmentMessage).toBeDefined()
        expect(
          remoteAttachmentMessage!.content.remoteAttachment?.filename
        ).toBeUndefined()
      })
    })

    describe('Multi Remote Attachment', () => {
      it('should send and receive multi remote attachment', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const multiRemoteAttachment: MultiRemoteAttachment = {
          attachments: [
            {
              url: 'https://example.com/file1.png',
              contentDigest: 'abc123',
              secret: new Uint8Array([1, 2, 3]),
              salt: new Uint8Array([4, 5, 6]),
              nonce: new Uint8Array([7, 8, 9]),
              scheme: 'https',
              contentLength: 1000,
              filename: 'file1.png',
            },
            {
              url: 'https://example.com/file2.pdf',
              contentDigest: 'def456',
              secret: new Uint8Array([10, 11, 12]),
              salt: new Uint8Array([13, 14, 15]),
              nonce: new Uint8Array([16, 17, 18]),
              scheme: 'https',
              contentLength: 2000,
              filename: 'file2.pdf',
            },
          ],
        }

        const multiRemoteAttachmentId =
          await conversation.sendMultiRemoteAttachment(multiRemoteAttachment)
        expect(multiRemoteAttachmentId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const multiRemoteAttachmentMessage = messages.find(
          (m) => m.id === multiRemoteAttachmentId
        )
        expect(multiRemoteAttachmentMessage).toBeDefined()
        expect(multiRemoteAttachmentMessage!.content.type).toBe(
          DecodedMessageContentType.MultiRemoteAttachment
        )
        expect(
          multiRemoteAttachmentMessage!.content.multiRemoteAttachment
            ?.attachments.length
        ).toBe(2)
        expect(
          multiRemoteAttachmentMessage!.content.multiRemoteAttachment
            ?.attachments[0].filename
        ).toBe('file1.png')
        expect(
          multiRemoteAttachmentMessage!.content.multiRemoteAttachment
            ?.attachments[1].filename
        ).toBe('file2.pdf')
        expect(multiRemoteAttachmentMessage!.contentType?.typeId).toBe(
          'multiRemoteStaticAttachment'
        )
        // Multi remote attachment fallback
        expect(multiRemoteAttachmentMessage!.fallback).toBe(
          `Can't display this content. This app doesn't support multiple remote attachments.`
        )
      })

      it('should send and receive multi remote attachment with single attachment', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const multiRemoteAttachment: MultiRemoteAttachment = {
          attachments: [
            {
              url: 'https://example.com/single.png',
              contentDigest: 'single123',
              secret: new Uint8Array([1, 2, 3]),
              salt: new Uint8Array([4, 5, 6]),
              nonce: new Uint8Array([7, 8, 9]),
              scheme: 'https',
              contentLength: 500,
              filename: 'single.png',
            },
          ],
        }

        const multiRemoteAttachmentId =
          await conversation.sendMultiRemoteAttachment(multiRemoteAttachment)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const multiRemoteAttachmentMessage = messages.find(
          (m) => m.id === multiRemoteAttachmentId
        )
        expect(multiRemoteAttachmentMessage).toBeDefined()
        expect(
          multiRemoteAttachmentMessage!.content.multiRemoteAttachment
            ?.attachments.length
        ).toBe(1)
      })
    })

    describe('Read Receipt', () => {
      it('should send read receipt (excluded from enriched messages by design)', async () => {
        const { conversation } = await setupConversation()

        const receiptId = await conversation.sendReadReceipt()
        expect(receiptId).toBeDefined()

        // Read receipts are excluded from enriched messages by design
        const messages = await conversation.findEnrichedMessages()
        const receiptMessage = messages.find((m) => m.id === receiptId)
        expect(receiptMessage).toBeUndefined()
      })
    })

    describe('Transaction Reference', () => {
      it('should send and receive transaction reference', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const transactionReference: TransactionReference = {
          namespace: 'eip155',
          networkId: '1',
          reference: '0x1234567890abcdef',
        }

        const transactionReferenceId =
          await conversation.sendTransactionReference(transactionReference)
        expect(transactionReferenceId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const transactionReferenceMessage = messages.find(
          (m) => m.id === transactionReferenceId
        )
        expect(transactionReferenceMessage).toBeDefined()
        expect(transactionReferenceMessage!.content.type).toBe(
          DecodedMessageContentType.TransactionReference
        )
        expect(
          transactionReferenceMessage!.content.transactionReference?.namespace
        ).toBe('eip155')
        expect(
          transactionReferenceMessage!.content.transactionReference?.networkId
        ).toBe('1')
        expect(
          transactionReferenceMessage!.content.transactionReference?.reference
        ).toBe('0x1234567890abcdef')
        expect(transactionReferenceMessage!.contentType?.typeId).toBe(
          'transactionReference'
        )
        // Transaction reference fallback with reference
        expect(transactionReferenceMessage!.fallback).toBe(
          `[Crypto transaction] Use a blockchain explorer to learn more using the transaction hash: 0x1234567890abcdef`
        )
      })

      it('should send and receive transaction reference without namespace', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const transactionReference: TransactionReference = {
          networkId: '137',
          reference: '0xabcdef',
        }

        const transactionReferenceId =
          await conversation.sendTransactionReference(transactionReference)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const transactionReferenceMessage = messages.find(
          (m) => m.id === transactionReferenceId
        )
        expect(transactionReferenceMessage).toBeDefined()
        expect(
          transactionReferenceMessage!.content.transactionReference?.namespace
        ).toBeUndefined()
        expect(
          transactionReferenceMessage!.content.transactionReference?.networkId
        ).toBe('137')
      })

      it('should send and receive transaction reference with empty reference', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const transactionReference: TransactionReference = {
          namespace: 'eip155',
          networkId: '1',
          reference: '',
        }

        const transactionReferenceId =
          await conversation.sendTransactionReference(transactionReference)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const transactionReferenceMessage = messages.find(
          (m) => m.id === transactionReferenceId
        )
        expect(transactionReferenceMessage).toBeDefined()
        // Transaction reference fallback without reference
        expect(transactionReferenceMessage!.fallback).toBe(`Crypto transaction`)
      })

      it('should send and receive transaction reference with metadata', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const transactionReference: TransactionReference = {
          namespace: 'eip155',
          networkId: '1',
          reference: '0x123',
          metadata: {
            transactionType: 'transfer',
            currency: 'ETH',
            amount: 1.5,
            decimals: 18,
            fromAddress: '0xabc',
            toAddress: '0xdef',
          },
        }

        const transactionReferenceId =
          await conversation.sendTransactionReference(transactionReference)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const transactionReferenceMessage = messages.find(
          (m) => m.id === transactionReferenceId
        )
        expect(transactionReferenceMessage).toBeDefined()
        expect(
          transactionReferenceMessage!.content.transactionReference?.metadata
        ).toBeDefined()
        expect(
          transactionReferenceMessage!.content.transactionReference?.metadata
            ?.transactionType
        ).toBe('transfer')
        expect(
          transactionReferenceMessage!.content.transactionReference?.metadata
            ?.currency
        ).toBe('ETH')
        expect(
          transactionReferenceMessage!.content.transactionReference?.metadata
            ?.amount
        ).toBe(1.5)
      })
    })

    describe('Wallet Send Calls', () => {
      it('should send and receive wallet send calls', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const walletSendCalls: WalletSendCalls = {
          version: '1',
          chainId: '1',
          from: '0x1234567890abcdef1234567890abcdef12345678',
          calls: [
            {
              to: '0xabcdef1234567890abcdef1234567890abcdef12',
              data: '0x',
              value: '0x0',
            },
          ],
        }

        const walletSendCallsId =
          await conversation.sendWalletSendCalls(walletSendCalls)
        expect(walletSendCallsId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const walletSendCallsMessage = messages.find(
          (m) => m.id === walletSendCallsId
        )
        expect(walletSendCallsMessage).toBeDefined()
        expect(walletSendCallsMessage!.content.type).toBe(
          DecodedMessageContentType.WalletSendCalls
        )
        expect(walletSendCallsMessage!.content.walletSendCalls?.version).toBe(
          '1'
        )
        expect(walletSendCallsMessage!.content.walletSendCalls?.chainId).toBe(
          '1'
        )
        expect(walletSendCallsMessage!.content.walletSendCalls?.from).toBe(
          '0x1234567890abcdef1234567890abcdef12345678'
        )
        expect(
          walletSendCallsMessage!.content.walletSendCalls?.calls.length
        ).toBe(1)
        expect(walletSendCallsMessage!.contentType?.typeId).toBe(
          'walletSendCalls'
        )
      })

      it('should send and receive wallet send calls with multiple calls', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const walletSendCalls: WalletSendCalls = {
          version: '1',
          chainId: '137',
          from: '0x1234',
          calls: [
            {
              to: '0xabc',
              data: '0x123',
              value: '0x1',
            },
            {
              to: '0xdef',
              data: '0x456',
              value: '0x2',
              gas: '0x5208',
            },
          ],
        }

        const walletSendCallsId =
          await conversation.sendWalletSendCalls(walletSendCalls)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const walletSendCallsMessage = messages.find(
          (m) => m.id === walletSendCallsId
        )
        expect(walletSendCallsMessage).toBeDefined()
        expect(
          walletSendCallsMessage!.content.walletSendCalls?.calls.length
        ).toBe(2)
        expect(
          walletSendCallsMessage!.content.walletSendCalls?.calls[0].to
        ).toBe('0xabc')
        expect(
          walletSendCallsMessage!.content.walletSendCalls?.calls[1].gas
        ).toBe('0x5208')
      })

      it('should send and receive wallet send calls with metadata', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const walletSendCalls: WalletSendCalls = {
          version: '1',
          chainId: '1',
          from: '0x1234',
          calls: [
            {
              to: '0xabc',
              data: '0x',
              value: '0x0',
              metadata: {
                description: 'Send funds',
                transactionType: 'transfer',
                note: 'test payment',
              },
            },
          ],
          capabilities: {
            paymasterService: 'https://paymaster.example.com',
          },
        }

        const walletSendCallsId =
          await conversation.sendWalletSendCalls(walletSendCalls)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const walletSendCallsMessage = messages.find(
          (m) => m.id === walletSendCallsId
        )
        expect(walletSendCallsMessage).toBeDefined()
        const metadata =
          walletSendCallsMessage!.content.walletSendCalls?.calls[0].metadata
        expect(metadata?.description).toBe('Send funds')
        expect(metadata?.transactionType).toBe('transfer')
        expect(metadata?.note).toBe('test payment')
        expect(
          walletSendCallsMessage!.content.walletSendCalls?.capabilities
            ?.paymasterService
        ).toBe('https://paymaster.example.com')
      })

      it('should error when metadata is missing `description` field', async () => {
        const { conversation } = await setupConversation()

        const walletSendCalls: WalletSendCalls = {
          version: '1',
          chainId: '1',
          from: '0x1234',
          calls: [
            {
              to: '0xabc',
              data: '0x',
              value: '0x0',
              metadata: {
                transactionType: 'transfer',
                note: 'test payment',
              },
            },
          ],
        }

        await expect(
          conversation.sendWalletSendCalls(walletSendCalls)
        ).rejects.toThrow('missing field `description`')
      })

      it('should error when metadata is missing `transactionType` field', async () => {
        const { conversation } = await setupConversation()

        const walletSendCalls: WalletSendCalls = {
          version: '1',
          chainId: '1',
          from: '0x1234',
          calls: [
            {
              to: '0xabc',
              data: '0x',
              value: '0x0',
              metadata: {
                description: 'Send funds',
                note: 'test payment',
              },
            },
          ],
        }

        await expect(
          conversation.sendWalletSendCalls(walletSendCalls)
        ).rejects.toThrow('missing field `transactionType`')
      })
    })

    describe('Actions', () => {
      it('should send and receive actions', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const actions: Actions = {
          id: 'action-1',
          description: 'Choose an option',
          actions: [
            {
              id: 'opt-1',
              label: 'Option 1',
              style: ActionStyle.Primary,
            },
            {
              id: 'opt-2',
              label: 'Option 2',
              style: ActionStyle.Secondary,
            },
          ],
        }

        const actionsId = await conversation.sendActions(actions)
        expect(actionsId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const actionsMessage = messages.find((m) => m.id === actionsId)
        expect(actionsMessage).toBeDefined()
        expect(actionsMessage!.content.type).toBe(
          DecodedMessageContentType.Actions
        )
        expect(actionsMessage!.content.actions?.id).toBe('action-1')
        expect(actionsMessage!.content.actions?.description).toBe(
          'Choose an option'
        )
        expect(actionsMessage!.content.actions?.actions.length).toBe(2)
        expect(actionsMessage!.content.actions?.actions[0].label).toBe(
          'Option 1'
        )
        expect(actionsMessage!.content.actions?.actions[0].style).toBe(
          ActionStyle.Primary
        )
        expect(actionsMessage!.contentType?.authorityId).toBe('coinbase.com')
        expect(actionsMessage!.contentType?.typeId).toBe('actions')
        // Actions fallback
        expect(actionsMessage!.fallback).toBe(
          `Choose an option\n\n[1] Option 1\n[2] Option 2\n\nReply with the number to select`
        )
      })

      it('should send and receive actions with all styles', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const actions: Actions = {
          id: 'action-styles',
          description: 'All styles',
          actions: [
            {
              id: 'primary',
              label: 'Primary',
              style: ActionStyle.Primary,
            },
            {
              id: 'secondary',
              label: 'Secondary',
              style: ActionStyle.Secondary,
            },
            {
              id: 'danger',
              label: 'Danger',
              style: ActionStyle.Danger,
            },
          ],
        }

        const actionsId = await conversation.sendActions(actions)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const actionsMessage = messages.find((m) => m.id === actionsId)
        expect(actionsMessage).toBeDefined()
        expect(actionsMessage!.content.actions?.actions[0].style).toBe(
          ActionStyle.Primary
        )
        expect(actionsMessage!.content.actions?.actions[1].style).toBe(
          ActionStyle.Secondary
        )
        expect(actionsMessage!.content.actions?.actions[2].style).toBe(
          ActionStyle.Danger
        )
      })

      it('should send and receive actions with expiration', async () => {
        const { conversation, conversation2 } = await setupConversation()

        // Use a timestamp in nanoseconds (must fit in i64)
        const expiresAtNs = 1700000000000000000n // Nov 2023 in nanoseconds

        const actions: Actions = {
          id: 'expiring-action',
          description: 'Expiring action',
          actions: [
            {
              id: 'opt-1',
              label: 'Option 1',
              style: ActionStyle.Primary,
              expiresAtNs: expiresAtNs,
            },
          ],
          expiresAtNs: expiresAtNs,
        }

        const actionsId = await conversation.sendActions(actions)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const actionsMessage = messages.find((m) => m.id === actionsId)
        expect(actionsMessage).toBeDefined()
        expect(actionsMessage!.content.actions?.expiresAtNs).toBeDefined()
        expect(
          actionsMessage!.content.actions?.actions[0].expiresAtNs
        ).toBeDefined()
      })

      it('should send and receive actions with image URL', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const actions: Actions = {
          id: 'action-with-image',
          description: 'Action with image',
          actions: [
            {
              id: 'opt-1',
              label: 'Option 1',
              style: ActionStyle.Primary,
              imageUrl: 'https://example.com/image.png',
            },
          ],
        }

        const actionsId = await conversation.sendActions(actions)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const actionsMessage = messages.find((m) => m.id === actionsId)
        expect(actionsMessage).toBeDefined()
        expect(actionsMessage!.content.actions?.actions[0].imageUrl).toBe(
          'https://example.com/image.png'
        )
      })
    })

    describe('Intent', () => {
      it('should send and receive intent', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const intent: Intent = {
          id: 'intent-1',
          actionId: 'opt-1',
        }

        const intentId = await conversation.sendIntent(intent)
        expect(intentId).toBeDefined()

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const intentMessage = messages.find((m) => m.id === intentId)
        expect(intentMessage).toBeDefined()
        expect(intentMessage!.content.type).toBe(
          DecodedMessageContentType.Intent
        )
        expect(intentMessage!.content.intent?.id).toBe('intent-1')
        expect(intentMessage!.content.intent?.actionId).toBe('opt-1')
        expect(intentMessage!.contentType?.authorityId).toBe('coinbase.com')
        expect(intentMessage!.contentType?.typeId).toBe('intent')
        // Intent fallback
        expect(intentMessage!.fallback).toBe(`User selected action: opt-1`)
      })

      it('should send and receive intent with metadata', async () => {
        const { conversation, conversation2 } = await setupConversation()

        const intent: Intent = {
          id: 'intent-2',
          actionId: 'opt-2',
          metadata: {
            source: 'test',
            timestamp: '2024-01-01',
          },
        }

        const intentId = await conversation.sendIntent(intent)

        await conversation2.sync()

        const messages = await conversation2.findEnrichedMessages()
        const intentMessage = messages.find((m) => m.id === intentId)
        expect(intentMessage).toBeDefined()
        expect(intentMessage!.content.intent?.metadata).toBeDefined()
        expect(intentMessage!.content.intent?.metadata?.source).toBe('test')
        expect(intentMessage!.content.intent?.metadata?.timestamp).toBe(
          '2024-01-01'
        )
      })
    })

    describe('Group Updated', () => {
      it('should include group updated messages when members are added', async () => {
        const user1 = createUser()
        const user2 = createUser()
        const user3 = createUser()
        const client1 = await createRegisteredClient(user1)
        await createRegisteredClient(user2)
        await createRegisteredClient(user3)

        const conversation = await client1.conversations().createGroup([
          {
            identifier: user2.account.address,
            identifierKind: IdentifierKind.Ethereum,
          },
        ])

        await conversation.addMembers([
          {
            identifier: user3.account.address,
            identifierKind: IdentifierKind.Ethereum,
          },
        ])

        const messages = await conversation.findEnrichedMessages()
        const groupUpdatedMessages = messages.filter(
          (m) => m.content.groupUpdated !== null
        )
        expect(groupUpdatedMessages).toHaveLength(2)

        const lastUpdate = groupUpdatedMessages[groupUpdatedMessages.length - 1]
        expect(lastUpdate.content.type).toBe(
          DecodedMessageContentType.GroupUpdated
        )
        expect(lastUpdate.content.groupUpdated?.initiatedByInboxId).toBe(
          client1.inboxId()
        )
        expect(
          lastUpdate.content.groupUpdated?.addedInboxes.length
        ).toBeGreaterThan(0)
      })

      it('should include group updated messages when members are removed', async () => {
        const user1 = createUser()
        const user2 = createUser()
        const user3 = createUser()
        const client1 = await createRegisteredClient(user1)
        const client2 = await createRegisteredClient(user2)
        await createRegisteredClient(user3)

        const conversation = await client1.conversations().createGroup([
          {
            identifier: user2.account.address,
            identifierKind: IdentifierKind.Ethereum,
          },
          {
            identifier: user3.account.address,
            identifierKind: IdentifierKind.Ethereum,
          },
        ])

        await conversation.removeMembersByInboxId([client2.inboxId()])

        const messages = await conversation.findEnrichedMessages()
        const groupUpdatedMessages = messages.filter(
          (m) => m.content.groupUpdated !== null
        )
        expect(groupUpdatedMessages.length).toBeGreaterThanOrEqual(2)

        const removalUpdate = groupUpdatedMessages.find(
          (m) =>
            m.content.groupUpdated?.removedInboxes &&
            m.content.groupUpdated.removedInboxes.length > 0
        )
        expect(removalUpdate).toBeDefined()
        expect(
          removalUpdate!.content.groupUpdated?.removedInboxes[0].inboxId
        ).toBe(client2.inboxId())
      })

      it('should include group updated messages when metadata is changed', async () => {
        const user1 = createUser()
        const user2 = createUser()
        const client1 = await createRegisteredClient(user1)
        await createRegisteredClient(user2)

        const conversation = await client1.conversations().createGroup([
          {
            identifier: user2.account.address,
            identifierKind: IdentifierKind.Ethereum,
          },
        ])

        await conversation.updateGroupName('New Group Name')

        const messages = await conversation.findEnrichedMessages()
        const groupUpdatedMessages = messages.filter(
          (m) => m.content.groupUpdated !== null
        )

        const metadataUpdate = groupUpdatedMessages.find(
          (m) =>
            m.content.groupUpdated?.metadataFieldChanges &&
            m.content.groupUpdated.metadataFieldChanges.length > 0
        )
        expect(metadataUpdate).toBeDefined()
        expect(
          metadataUpdate!.content.groupUpdated?.metadataFieldChanges[0].newValue
        ).toBe('New Group Name')
      })
    })
  })
})
