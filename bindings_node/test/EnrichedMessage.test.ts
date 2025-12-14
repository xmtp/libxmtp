import { describe, expect, it } from 'vitest'
import { createRegisteredClient, createUser } from '@test/helpers'
import {
  DecodedMessage,
  EncodedContent,
  IdentifierKind,
  ReactionAction,
  ReactionCodec,
  ReactionSchema,
  ReplyCodec,
  SortDirection,
  TextCodec,
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
    return { user1, user2, client1, client2, conversation }
  }

  describe('Basic message retrieval', () => {
    it('should return enriched messages with basic fields populated', async () => {
      const { client1, conversation } = await setupConversation()

      await conversation.send(TextCodec.encode('Hello World'), {
        shouldPush: TextCodec.shouldPush(),
      })
      await conversation.send(TextCodec.encode('Second message'), {
        shouldPush: TextCodec.shouldPush(),
      })

      const messages = await conversation.findEnrichedMessages()
      expect(messages.length).toEqual(3)

      const textMessages = messages.filter((m) => m.textContent !== null)
      expect(textMessages.length).toEqual(2)

      const helloWorldMessage = textMessages.find(
        (m) => m.textContent?.content === 'Hello World'
      )
      expect(helloWorldMessage).toBeDefined()
      expect(helloWorldMessage!.id).toBeDefined()
      expect(helloWorldMessage!.sentAtNs).toBeDefined()
      expect(helloWorldMessage!.senderInboxId).toBe(client1.inboxId())
      expect(helloWorldMessage!.conversationId).toBeDefined()
      expect(helloWorldMessage!.textContent).toBeDefined()
      expect(helloWorldMessage!.textContent?.content).toBe('Hello World')
      expect(helloWorldMessage!.deliveryStatus).toBeDefined()
    })

    it('should handle list options', async () => {
      const { conversation } = await setupConversation()

      await conversation.send(TextCodec.encode('Message 1'), {
        shouldPush: TextCodec.shouldPush(),
      })
      await conversation.send(TextCodec.encode('Message 2'), {
        shouldPush: TextCodec.shouldPush(),
      })
      await conversation.send(TextCodec.encode('Message 3'), {
        shouldPush: TextCodec.shouldPush(),
      })

      const limitedMessages = await conversation.findEnrichedMessages({
        limit: 2,
        direction: SortDirection.Descending,
      })
      const limitedTextMessages = limitedMessages.filter(
        (m) => m.textContent !== null
      )
      expect(limitedTextMessages.length).toBe(2)

      const allMessages = await conversation.findEnrichedMessages()
      const allTextMessages = allMessages.filter((m) => m.textContent !== null)
      expect(allTextMessages.length).toEqual(3)
    })
  })

  describe('Message metadata', () => {
    it('should include message kind', async () => {
      const { conversation } = await setupConversation()

      await conversation.send(TextCodec.encode('Test'), {
        shouldPush: TextCodec.shouldPush(),
      })

      const messages = await conversation.findEnrichedMessages()

      expect(messages.length).toEqual(2)
      // Messages should have kinds defined
      const messagesWithKind = messages.filter((m) => m.kind !== undefined)
      expect(messagesWithKind.length).toEqual(2)
    })
  })

  describe('Content types', () => {
    type TestCase = {
      name: string
      content: EncodedContent
      assertions: (message: DecodedMessage) => void
    }

    const fixtures = {
      messageText: 'Test message',
      fileName: 'image.png',
      mimeType: 'image/png',
      contentDigest: 'abc123',
      secret: Buffer.from('123456').toString('hex'),
      walletAddress: '0x1234',
      reference: '0x1234567890abcdef',
      chainId: '1',
      authorityId: 'xmtp.org',
      remoteAttachmentUrl: 'https://example.com/files/document.pdf',
      remoteAttachmentFile: 'document.pdf',
    }

    const contentTypeTestCases: TestCase[] = [
      {
        name: 'text content',
        content: TextCodec.encode(fixtures.messageText),
        assertions: (message: DecodedMessage) => {
          expect(message.textContent).toBeDefined()
          expect(message.textContent?.content).toBe(fixtures.messageText)
          expect(message.contentType?.authorityId).toBe(fixtures.authorityId)
          expect(message.contentType?.typeId).toBe('text')
          expect(message.contentType?.versionMajor).toBe(1)
          expect(message.contentType?.versionMinor).toBe(0)
        },
      },
      {
        name: 'attachment content',
        content: {
          type: {
            authorityId: fixtures.authorityId,
            typeId: 'attachment',
            versionMajor: 1,
            versionMinor: 0,
          },
          parameters: {
            filename: fixtures.fileName,
            mimeType: fixtures.mimeType,
          },
          fallback: fixtures.fileName,
          content: Buffer.from('fake-image-data'),
        },
        assertions: (message: DecodedMessage) => {
          expect(message.attachmentContent).toBeDefined()
          expect(message.contentType.typeId).toBe('attachment')
          expect(message.fallbackText).toBe(fixtures.fileName)
          expect(message.attachmentContent?.mimeType).toBe(fixtures.mimeType)
        },
      },
      {
        name: 'remote attachment content',
        content: {
          type: {
            authorityId: fixtures.authorityId,
            typeId: 'remoteStaticAttachment',
            versionMajor: 1,
            versionMinor: 0,
          },
          parameters: {
            contentDigest: fixtures.contentDigest,
            secret: fixtures.secret,
            salt: 'encryption-salt',
            nonce: 'encryption-nonce',
            filename: fixtures.remoteAttachmentFile,
            scheme: 'https',
            contentLength: '123',
          },
          fallback: fixtures.remoteAttachmentFile,
          content: new TextEncoder().encode(fixtures.remoteAttachmentUrl),
        },
        assertions: (message: DecodedMessage) => {
          expect(message.remoteAttachmentContent).toBeDefined()
          expect(message.fallbackText).toBe(fixtures.remoteAttachmentFile)
          expect(message.remoteAttachmentContent?.filename).toBe(
            fixtures.remoteAttachmentFile
          )
          expect(Buffer.from(message.remoteAttachmentContent!.secret)).toEqual(
            Buffer.from(fixtures.secret, 'hex')
          )
          expect(message.remoteAttachmentContent?.contentDigest).toBe(
            fixtures.contentDigest
          )
        },
      },
      {
        name: 'wallet send call content',
        content: {
          type: {
            authorityId: 'xmtp.org',
            typeId: 'walletSendCalls',
            versionMajor: 1,
            versionMinor: 0,
          },
          parameters: {},
          fallback: 'Transaction: 0x1234567890abcdef',
          content: new TextEncoder().encode(
            JSON.stringify({
              version: '1',
              chainId: '1',
              from: fixtures.walletAddress,
              calls: [
                {
                  to: '0x123',
                  data: '0x123',
                  value: '0x0',
                },
              ],
            })
          ),
        },
        assertions: (message: DecodedMessage) => {
          expect(message.walletSendCallsContent).toBeDefined()
          expect(message.contentType.authorityId).toBe(fixtures.authorityId)
          expect(message.contentType.typeId).toBe('walletSendCalls')
          expect(message.fallbackText).toBe('Transaction: 0x1234567890abcdef')
          expect(message.walletSendCallsContent?.chainId).toBe('1')
          expect(message.walletSendCallsContent?.calls).toHaveLength(1)
          expect(message.walletSendCallsContent?.calls[0].to).toBe('0x123')
        },
      },
      {
        name: 'custom content',
        content: {
          type: {
            authorityId: 'example.com',
            typeId: 'custom',
            versionMajor: 1,
            versionMinor: 0,
          },
          parameters: {},
          content: new TextEncoder().encode('Custom content'),
        },
        assertions: (message: any) => {
          expect(message.customContent).toBeDefined()
          expect(message.customContent?.type?.authorityId).toBe('example.com')
          expect(message.customContent?.type?.typeId).toBe('custom')
        },
      },
      {
        name: 'custom content with fallback',
        content: {
          type: {
            authorityId: 'example.com',
            typeId: 'custom-fallback',
            versionMajor: 1,
            versionMinor: 0,
          },
          parameters: {},
          fallback: 'This is a fallback message',
          content: new TextEncoder().encode(fixtures.messageText),
        },
        assertions: (message: DecodedMessage) => {
          expect(message.fallbackText).toBe('This is a fallback message')
          expect(message.customContent?.content).toEqual(
            new Uint8Array(new TextEncoder().encode(fixtures.messageText))
          )
        },
      },
    ]

    contentTypeTestCases.forEach(({ name, content, assertions }) => {
      it(`should handle ${name}`, async () => {
        const { conversation } = await setupConversation()

        await conversation.send(content as EncodedContent, { shouldPush: true })

        const messages = await conversation.findEnrichedMessages()
        const matchingMessages = messages.filter(
          (m: DecodedMessage) => m.contentType.typeId === content.type?.typeId
        )

        expect(matchingMessages).toHaveLength(1)
        // Get the first matching message
        const message = matchingMessages[0]
        expect(message).toBeDefined()
        assertions(message)
      })
    })

    // Separate tests for reactions and replies as they need context
    it('should handle reaction content in reactions array', async () => {
      const { client1, conversation } = await setupConversation()

      // Send a text message first to react to
      await conversation.send(TextCodec.encode('Original message'), {
        shouldPush: TextCodec.shouldPush(),
      })
      const messagesBefore = await conversation.findEnrichedMessages()
      const textMessage = messagesBefore.find(
        (m) => m.textContent?.content === 'Original message'
      )
      expect(textMessage).toBeDefined()
      expect(textMessage!.reactions.length).toBe(0)

      // Send a reaction to the text message
      const messageIdHex = Buffer.from(textMessage!.id).toString('hex')
      const reactionContent = ReactionCodec.encode({
        reference: messageIdHex,
        referenceInboxId: client1.inboxId(),
        action: ReactionAction.Added,
        content: '👍',
        schema: ReactionSchema.Unicode,
      })
      await conversation.send(reactionContent, {
        shouldPush: ReactionCodec.shouldPush(),
      })

      // Sync to ensure the reaction is processed
      await conversation.sync()

      // Reactions should NOT increase the message count
      // They should appear in the reactions array of the original message
      const messagesAfter = await conversation.findEnrichedMessages()
      expect(messagesAfter.length).toBe(messagesBefore.length)

      // Find the original message again and check its reactions array
      const textMessageWithReaction = messagesAfter.find(
        (m) => m.textContent?.content === 'Original message'
      )
      expect(textMessageWithReaction).toBeDefined()

      // Verify the reactions array is populated
      expect(textMessageWithReaction!.reactions).toHaveLength(1)

      // Verify the reaction details
      const reaction = textMessageWithReaction!.reactions[0]
      expect(reaction.reactionContent).toBeDefined()
      expect(reaction.reactionContent?.content).toBe('👍')
      expect(reaction.reactionContent?.action).toBe(ReactionAction.Added)
      expect(reaction.reactionContent?.schema).toBe(ReactionSchema.Unicode)
      expect(reaction.senderInboxId).toBe(client1.inboxId())
    })

    it('should handle reply content', async () => {
      const { conversation } = await setupConversation()

      // Send a text message first to reply to
      const textMessageID = await conversation.send(
        TextCodec.encode('Original message'),
        { shouldPush: TextCodec.shouldPush() }
      )
      const messages = await conversation.findEnrichedMessages()
      const textMessage = messages.find(
        (m) => m.textContent?.content === 'Original message'
      )
      expect(textMessage).toBeDefined()

      // Send a reply to the text message
      const replyContent = ReplyCodec.encode({
        reference: textMessageID,
        content: TextCodec.encode('This is a reply'),
      })
      // Verify we can send the reply message
      const replyId = await conversation.send(replyContent, {
        shouldPush: true,
      })
      expect(replyId).toBeDefined()

      // Verify the message count increased
      const allMessages = await conversation.findEnrichedMessages()
      expect(allMessages.length).toEqual(3)

      const replyMessage = allMessages.find(
        (m) => m.contentType.typeId === 'reply'
      )
      expect(replyMessage).toBeDefined()
      expect(replyMessage?.replyContent).toBeDefined()
      expect(replyMessage?.replyContent?.content.textContent?.content).toEqual(
        'This is a reply'
      )

      expect(replyMessage?.replyContent?.inReplyTo).toBeDefined()
      expect(
        replyMessage?.replyContent?.inReplyTo?.textContent?.content
      ).toEqual('Original message')
    })
  })

  describe('Group operations', () => {
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
        (m) => m.groupUpdatedContent !== null
      )
      expect(groupUpdatedMessages).toHaveLength(2)

      const lastUpdate = groupUpdatedMessages[groupUpdatedMessages.length - 1]
      expect(lastUpdate.groupUpdatedContent?.initiatedByInboxId).toBe(
        client1.inboxId()
      )
    })
  })
})
