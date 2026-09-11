import { v4 } from 'uuid'
import { describe, expect, it } from 'vitest'
import {
  createRegisteredClient,
  createToxicRegisteredClient,
  createUser,
  sleep,
} from '@test/helpers'
import {
  ConsentState,
  contentTypeGroupUpdated,
  contentTypeText,
  Conversation,
  ConversationType,
  DecodedMessage,
  GroupMessageKind,
  GroupPermissionsOptions,
  IdentifierKind,
  Message,
  MetadataField,
  PermissionPolicy,
  PermissionUpdateType,
} from '../dist'

// The connection-death test below uses the h2 transport keepalive to find a
// black-holed connection. Set it to a short interval before any client exists,
// because the Rust side reads these values one time for each process and
// vitest gives each test file its own. Detection then occurs well inside the
// wait windows of the test, whatever the library defaults are.
process.env.XMTP_GRPC_KEEPALIVE_INTERVAL_SECS = '10'
process.env.XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS = '10'

const expectStreamedMessages = (
  messages: Message[],
  applicationMessages: [string, string][],
  membershipGroupIds: string[]
) => {
  expect(messages).toHaveLength(
    applicationMessages.length + membershipGroupIds.length
  )
  expect(new Set(messages.map((message) => message.id)).size).toBe(
    messages.length
  )
  expect(
    messages
      .filter((message) => message.kind === GroupMessageKind.Application)
      .map((message) => [
        message.id,
        new TextDecoder().decode(message.content.content),
      ])
      .sort()
  ).toEqual([...applicationMessages].sort())
  const membership = messages.filter(
    (message) => message.kind === GroupMessageKind.MembershipChange
  )
  expect(membership.map((message) => message.convoId).sort()).toEqual(
    [...membershipGroupIds].sort()
  )
  for (const message of membership) {
    expect(message.content.type).toEqual(contentTypeGroupUpdated())
  }
}

describe('Conversations', () => {
  it('should not have initial conversations', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)

    expect(client.conversations().list().length).toBe(0)
  })

  it('should create a group chat', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    expect(group).toBeDefined()
    expect(group.id()).toBeDefined()
    expect(group.createdAtNs()).toBeTypeOf('bigint')
    expect(group.isActive()).toBe(true)
    expect(group.groupName()).toBe('')
    expect(group.groupPermissions().policyType()).toBe(
      GroupPermissionsOptions.Default
    )
    expect(group.groupPermissions().policySet()).toEqual({
      addMemberPolicy: 0,
      removeMemberPolicy: 2,
      addAdminPolicy: 3,
      removeAdminPolicy: 3,
      updateAppDataPolicy: 0,
      updateGroupNamePolicy: 0,
      updateGroupDescriptionPolicy: 0,
      updateGroupImageUrlSquarePolicy: 0,
      updateMessageDisappearingPolicy: 2,
    })
    expect(group.addedByInboxId()).toBe(client1.inboxId())
    expect((await group.listMessages()).length).toBe(1)
    const members = await group.listMembers()
    expect(members.length).toBe(2)
    const memberInboxIds = members.map((member) => member.inboxId)
    expect(memberInboxIds).toContain(client1.inboxId())
    expect(memberInboxIds).toContain(client2.inboxId())
    expect((await group.groupMetadata()).conversationType()).toBe(
      ConversationType.Group
    )
    expect((await group.groupMetadata()).creatorInboxId()).toBe(
      client1.inboxId()
    )

    expect(group.consentState()).toBe(ConsentState.Allowed)

    const groups1 = client1.conversations().list()
    expect(groups1.length).toBe(1)
    expect(groups1[0].conversation.id()).toBe(group.id())

    expect(
      client1.conversations().list({ conversationType: ConversationType.Dm })
        .length
    ).toBe(0)
    expect(
      client1.conversations().list({ conversationType: ConversationType.Group })
        .length
    ).toBe(1)

    expect(client2.conversations().list().length).toBe(0)

    await client2.conversations().sync()

    const groups2 = client2.conversations().list()
    expect(groups2.length).toBe(1)
    expect(groups2[0].conversation.id()).toBe(group.id())

    expect(
      client2.conversations().list({ conversationType: ConversationType.Dm })
        .length
    ).toBe(0)
    expect(
      client2.conversations().list({ conversationType: ConversationType.Group })
        .length
    ).toBe(1)
  })

  it('should create a group with custom permissions', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1.conversations().createGroupByIdentity(
      [
        {
          identifier: user2.account.address,
          identifierKind: IdentifierKind.Ethereum,
        },
      ],
      {
        permissions: GroupPermissionsOptions.CustomPolicy,
        customPermissionPolicySet: {
          addAdminPolicy: 2,
          addMemberPolicy: 3,
          removeAdminPolicy: 1,
          removeMemberPolicy: 0,
          updateAppDataPolicy: 1,
          updateGroupNamePolicy: 2,
          updateGroupDescriptionPolicy: 1,
          updateGroupImageUrlSquarePolicy: 0,
          updateMessageDisappearingPolicy: 2,
        },
      }
    )
    expect(group).toBeDefined()
    expect(group.groupPermissions().policyType()).toBe(
      GroupPermissionsOptions.CustomPolicy
    )
    expect(group.groupPermissions().policySet()).toEqual({
      addAdminPolicy: 2,
      addMemberPolicy: 3,
      removeAdminPolicy: 1,
      removeMemberPolicy: 0,
      updateAppDataPolicy: 1,
      updateGroupNamePolicy: 2,
      updateGroupDescriptionPolicy: 1,
      updateGroupImageUrlSquarePolicy: 0,
      updateMessageDisappearingPolicy: 2,
    })
  })

  it('should update group permission policy', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])

    expect(group.groupPermissions().policySet()).toEqual({
      addMemberPolicy: 0,
      removeMemberPolicy: 2,
      addAdminPolicy: 3,
      removeAdminPolicy: 3,
      updateAppDataPolicy: 0,
      updateGroupNamePolicy: 0,
      updateGroupDescriptionPolicy: 0,
      updateGroupImageUrlSquarePolicy: 0,
      updateMessageDisappearingPolicy: 2,
    })

    await group.updatePermissionPolicy(
      PermissionUpdateType.AddAdmin,
      PermissionPolicy.Deny
    )

    expect(group.groupPermissions().policySet()).toEqual({
      addMemberPolicy: 0,
      removeMemberPolicy: 2,
      addAdminPolicy: 1,
      removeAdminPolicy: 3,
      updateAppDataPolicy: 0,
      updateGroupNamePolicy: 0,
      updateGroupDescriptionPolicy: 0,
      updateGroupImageUrlSquarePolicy: 0,
      updateMessageDisappearingPolicy: 2,
    })

    await group.updatePermissionPolicy(
      PermissionUpdateType.UpdateMetadata,
      PermissionPolicy.Deny,
      MetadataField.GroupName
    )

    expect(group.groupPermissions().policySet()).toEqual({
      addMemberPolicy: 0,
      removeMemberPolicy: 2,
      addAdminPolicy: 1,
      removeAdminPolicy: 3,
      updateAppDataPolicy: 0,
      updateGroupNamePolicy: 1,
      updateGroupDescriptionPolicy: 0,
      updateGroupImageUrlSquarePolicy: 0,
      updateMessageDisappearingPolicy: 2,
    })
  })

  it('should create a dm group', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1.conversations().createDmByIdentity({
      identifier: user2.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
    expect(group).toBeDefined()
    expect(group.id()).toBeDefined()
    expect(group.createdAtNs()).toBeTypeOf('bigint')
    expect(group.isActive()).toBe(true)
    expect(group.groupName()).toBe('')
    expect(group.groupPermissions().policyType()).toBe(
      GroupPermissionsOptions.CustomPolicy
    )
    expect(group.groupPermissions().policySet()).toEqual({
      addAdminPolicy: 1,
      addMemberPolicy: 1,
      removeAdminPolicy: 1,
      removeMemberPolicy: 1,
      updateAppDataPolicy: 0,
      updateGroupDescriptionPolicy: 0,
      updateGroupImageUrlSquarePolicy: 0,
      updateGroupNamePolicy: 0,
      updateMessageDisappearingPolicy: 0,
    })
    expect(group.addedByInboxId()).toBe(client1.inboxId())
    expect((await group.listMessages()).length).toBe(1)
    const members = await group.listMembers()
    expect(members.length).toBe(2)
    const memberInboxIds = members.map((member) => member.inboxId)
    expect(memberInboxIds).toContain(client1.inboxId())
    expect(memberInboxIds).toContain(client2.inboxId())
    expect((await group.groupMetadata()).conversationType()).toBe(
      ConversationType.Dm
    )
    expect((await group.groupMetadata()).creatorInboxId()).toBe(
      client1.inboxId()
    )

    expect(group.consentState()).toBe(ConsentState.Allowed)

    const groups1 = client1.conversations().list()
    expect(groups1.length).toBe(1)
    expect(groups1[0].conversation.id()).toBe(group.id())
    expect(groups1[0].conversation.dmPeerInboxId()).toBe(client2.inboxId())

    expect(
      client1.conversations().list({ conversationType: ConversationType.Dm })
        .length
    ).toBe(1)
    expect(
      client1.conversations().list({ conversationType: ConversationType.Group })
        .length
    ).toBe(0)

    expect(client2.conversations().list().length).toBe(0)

    await client2.conversations().sync()

    const groups2 = client2.conversations().list()
    expect(groups2.length).toBe(1)
    expect(groups2[0].conversation.id()).toBe(group.id())
    expect(groups2[0].conversation.dmPeerInboxId()).toBe(client1.inboxId())

    expect(
      client2.conversations().list({ conversationType: ConversationType.Dm })
        .length
    ).toBe(1)
    expect(
      client2.conversations().list({ conversationType: ConversationType.Group })
        .length
    ).toBe(0)

    const dm1 = client1.conversations().getDmByInboxId(client2.inboxId())
    expect(dm1).toBeDefined()
    expect(dm1!.id()).toBe(group.id())

    const dm2 = client2.conversations().getDmByInboxId(client1.inboxId())
    expect(dm2).toBeDefined()
    expect(dm2!.id()).toBe(group.id())
  })

  it('should find a group by ID', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    expect(group).toBeDefined()
    expect(group.id()).toBeDefined()
    const foundGroup = client1.conversations().getConversationById(group.id())
    expect(foundGroup).toBeDefined()
    expect(foundGroup!.id()).toBe(group.id())
  })

  it('should find a message by ID', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    await createRegisteredClient(user2)
    const group = await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const messageId = await group.sendText('gm!')
    expect(messageId).toBeDefined()

    const message = client1.conversations().getMessageById(messageId)
    expect(message).toBeDefined()
    expect(message!.id).toBe(messageId)
  })

  it('should create a new group with options', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const user3 = createUser()
    const user4 = createUser()
    const user5 = createUser()
    const client1 = await createRegisteredClient(user1)
    await createRegisteredClient(user2)
    await createRegisteredClient(user3)
    await createRegisteredClient(user4)
    await createRegisteredClient(user5)
    const groupWithName = await client1.conversations().createGroupByIdentity(
      [
        {
          identifier: user2.account.address,
          identifierKind: IdentifierKind.Ethereum,
        },
      ],
      {
        groupName: 'foo',
      }
    )
    expect(groupWithName).toBeDefined()
    expect(groupWithName.groupName()).toBe('foo')
    expect(groupWithName.groupImageUrlSquare()).toBe('')

    const groupWithImageUrl = await client1
      .conversations()
      .createGroupByIdentity(
        [
          {
            identifier: user3.account.address,
            identifierKind: IdentifierKind.Ethereum,
          },
        ],
        {
          groupImageUrlSquare: 'https://foo/bar.png',
        }
      )
    expect(groupWithImageUrl).toBeDefined()
    expect(groupWithImageUrl.groupName()).toBe('')
    expect(groupWithImageUrl.groupImageUrlSquare()).toBe('https://foo/bar.png')

    const groupWithNameAndImageUrl = await client1
      .conversations()
      .createGroupByIdentity(
        [
          {
            identifier: user4.account.address,
            identifierKind: IdentifierKind.Ethereum,
          },
        ],
        {
          groupImageUrlSquare: 'https://foo/bar.png',
          groupName: 'foo',
        }
      )
    expect(groupWithNameAndImageUrl).toBeDefined()
    expect(groupWithNameAndImageUrl.groupName()).toBe('foo')
    expect(groupWithNameAndImageUrl.groupImageUrlSquare()).toBe(
      'https://foo/bar.png'
    )

    const groupWithPermissions = await client1
      .conversations()
      .createGroupByIdentity(
        [
          {
            identifier: user4.account.address,
            identifierKind: IdentifierKind.Ethereum,
          },
        ],
        {
          permissions: GroupPermissionsOptions.AdminOnly,
        }
      )
    expect(groupWithPermissions).toBeDefined()
    expect(groupWithPermissions.groupName()).toBe('')
    expect(groupWithPermissions.groupImageUrlSquare()).toBe('')
    expect(groupWithPermissions.groupPermissions().policyType()).toBe(
      GroupPermissionsOptions.AdminOnly
    )

    expect(groupWithPermissions.groupPermissions().policySet()).toEqual({
      addMemberPolicy: 2,
      removeMemberPolicy: 2,
      addAdminPolicy: 3,
      removeAdminPolicy: 3,
      updateAppDataPolicy: 2,
      updateGroupNamePolicy: 2,
      updateGroupDescriptionPolicy: 2,
      updateGroupImageUrlSquarePolicy: 2,
      updateMessageDisappearingPolicy: 2,
    })

    const groupWithDescription = await client1
      .conversations()
      .createGroupByIdentity(
        [
          {
            identifier: user2.account.address,
            identifierKind: IdentifierKind.Ethereum,
          },
        ],
        {
          groupDescription: 'foo',
        }
      )
    expect(groupWithDescription).toBeDefined()
    expect(groupWithDescription.groupName()).toBe('')
    expect(groupWithDescription.groupImageUrlSquare()).toBe('')
    expect(groupWithDescription.groupDescription()).toBe('foo')
  })

  it('should update group metadata', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    await createRegisteredClient(user2)
    const group = await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])

    await group.updateGroupName('foo')
    expect(group.groupName()).toBe('foo')

    await group.updateGroupImageUrlSquare('https://foo/bar.png')
    expect(group.groupImageUrlSquare()).toBe('https://foo/bar.png')

    await group.updateGroupDescription('bar')
    expect(group.groupDescription()).toBe('bar')
  })

  it('should stream all groups', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const user3 = createUser()
    const user4 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const client3 = await createRegisteredClient(user3)
    const client4 = await createRegisteredClient(user4)
    let groups: Conversation[] = []
    const stream = await client3.conversations().stream(
      (err, convo) => {
        groups.push(convo!)
      },
      () => {
        console.log('closed')
      }
    )
    const group1 = await client1.conversations().createGroupByIdentity([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group2 = await client2.conversations().createGroupByIdentity([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group3 = await client4.conversations().createDmByIdentity({
      identifier: user3.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    await sleep(2000)

    stream.end()
    expect(groups.length).toBe(3)
    expect(groups).toEqual([group1, group2, group3])
  })

  it(
    'should reconnect and resume after a black hole',
    { timeout: 60_000 },
    async () => {
      const user1 = createUser()
      const client2 = await createRegisteredClient(createUser())
      const client1 = await createToxicRegisteredClient(user1)
      const groups: Conversation[] = []
      const errors: Error[] = []
      let closed = false
      const startNewConvo = () =>
        client2.conversations().createGroupByIdentity([
          {
            identifier: user1.account.address,
            identifierKind: IdentifierKind.Ethereum,
          },
        ])
      const stream = await client1.client.conversations().stream(
        (error, convo) => {
          if (error) errors.push(error)
          if (convo) groups.push(convo)
        },
        () => {
          closed = true
        },
        ConversationType.Group
      )
      try {
        const first = await startNewConvo()
        await expect.poll(() => groups.length).toBe(1)
        await client1.withTimeout('downstream', 0, 1.0)
        const missed = await startNewConvo()
        // Allow both transport keepalive deadlines to expire before recovery.
        await sleep(30_000)
        expect(closed).toBe(false)
        await client1.deleteAllToxics()
        await expect.poll(() => groups.length, { timeout: 15_000 }).toBe(2)
        const after = await startNewConvo()
        await expect.poll(() => groups.length).toBe(3)
        expect(groups.map((group) => group.id())).toEqual([
          first.id(),
          missed.id(),
          after.id(),
        ])
        expect(errors).toEqual([])
        expect(closed).toBe(false)
      } finally {
        await client1.deleteAllToxics()
        stream.end()
      }
    }
  )

  it('should only stream group chats', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const user3 = createUser()
    const user4 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const client3 = await createRegisteredClient(user3)
    const client4 = await createRegisteredClient(user4)
    let groups: Conversation[] = []
    const stream = await client3.conversations().stream(
      (err, convo) => {
        groups.push(convo!)
      },
      () => {
        console.log('closed')
      },
      ConversationType.Group
    )
    const group3 = await client4.conversations().createDmByIdentity({
      identifier: user3.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
    const group1 = await client1.conversations().createGroupByIdentity([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group2 = await client2.conversations().createGroupByIdentity([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])

    await sleep(1000)

    stream.end()
    expect(groups.length).toBe(2)
    expect(groups).toEqual([group1, group2])
  })

  it('should only stream dm groups', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const user3 = createUser()
    const user4 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const client3 = await createRegisteredClient(user3)
    const client4 = await createRegisteredClient(user4)
    let groups: Conversation[] = []
    const stream = await client3.conversations().stream(
      (err, convo) => {
        groups.push(convo!)
      },
      () => {
        console.log('closed')
      },
      ConversationType.Dm
    )
    const group1 = await client1.conversations().createGroupByIdentity([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group2 = await client2.conversations().createGroupByIdentity([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group3 = await client4.conversations().createDmByIdentity({
      identifier: user3.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    await sleep(1000)

    stream.end()
    expect(groups.length).toBe(1)
    expect(groups).toEqual([group3])
  })

  it('should stream all messages', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const user3 = createUser()
    const user4 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const client3 = await createRegisteredClient(user3)
    const client4 = await createRegisteredClient(user4)
    const group1 = await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group2 = await client1.conversations().createGroupByIdentity([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const dm = await client1.conversations().createDmByIdentity({
      identifier: user4.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    await sleep(2000)

    const messages: Message[] = []
    const errors: Error[] = []
    const stream = await client1.conversations().streamAllMessages(
      (err, message) => {
        if (err) errors.push(err)
        if (message) messages.push(message)
      },
      () => {
        console.log('closed')
      },
      undefined,
      [ConsentState.Allowed, ConsentState.Unknown]
    )

    const messages2: Message[] = []
    const stream2 = await client2.conversations().streamAllMessages(
      (err, message) => {
        if (err) errors.push(err)
        if (message) messages2.push(message)
      },
      () => {
        console.log('closed')
      },
      undefined,
      [ConsentState.Allowed, ConsentState.Unknown]
    )

    const messages3: Message[] = []
    const stream3 = await client3.conversations().streamAllMessages(
      (err, message) => {
        if (err) errors.push(err)
        if (message) messages3.push(message)
      },
      () => {
        console.log('closed')
      },
      undefined,
      [ConsentState.Allowed, ConsentState.Unknown]
    )

    const messages4: Message[] = []
    const stream4 = await client4.conversations().streamAllMessages(
      (err, message) => {
        if (err) errors.push(err)
        if (message) messages4.push(message)
      },
      () => {
        console.log('closed')
      },
      undefined,
      [ConsentState.Allowed, ConsentState.Unknown]
    )

    const groups2 = client2.conversations()
    await groups2.sync()
    const groupsList2 = groups2.list()

    const groups3 = client3.conversations()
    await groups3.sync()
    const groupsList3 = groups3.list()

    const groups4 = client4.conversations()
    await groups4.sync()
    const groupsList4 = groups4.list()

    const message1 = await groupsList2[0].conversation.sendText('gm!')
    const message2 = await groupsList3[0].conversation.sendText('gm2!')
    const message3 = await groupsList4[0].conversation.sendText('gm3!')

    await expect
      .poll(
        () => [
          messages.length,
          messages2.length,
          messages3.length,
          messages4.length,
        ],
        {
          timeout: 15_000,
        }
      )
      .toEqual([6, 2, 2, 2])
    await Promise.all(
      [stream, stream2, stream3, stream4].map((value) => value.endAndWait())
    )
    expect(errors).toEqual([])
    expectStreamedMessages(
      messages,
      [
        [message1, 'gm!'],
        [message2, 'gm2!'],
        [message3, 'gm3!'],
      ],
      [group1.id(), group2.id(), dm.id()]
    )
    expectStreamedMessages(messages2, [[message1, 'gm!']], [group1.id()])
    expectStreamedMessages(messages3, [[message2, 'gm2!']], [group2.id()])
    expectStreamedMessages(messages4, [[message3, 'gm3!']], [dm.id()])
  })

  it('should only stream group chat messages', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const user3 = createUser()
    const user4 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const client3 = await createRegisteredClient(user3)
    const client4 = await createRegisteredClient(user4)
    const group1 = await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group2 = await client1.conversations().createGroupByIdentity([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client1.conversations().createDmByIdentity({
      identifier: user4.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    await sleep(2000)

    let messages: Message[] = []
    const errors: Error[] = []
    const stream = await client1.conversations().streamAllMessages(
      (err, message) => {
        if (err) errors.push(err)
        if (message) messages.push(message)
      },
      () => {
        console.log('closed')
      },
      ConversationType.Group
    )

    const groups2 = client2.conversations()
    await groups2.sync()
    const groupsList2 = groups2.list()

    const groups3 = client3.conversations()
    await groups3.sync()
    const groupsList3 = groups3.list()

    const groups4 = client4.conversations()
    await groups4.sync()
    const groupsList4 = groups4.list()

    await groupsList4[0].conversation.sendText('gm3!')
    const message1 = await groupsList2[0].conversation.sendText('gm!')
    const message2 = await groupsList3[0].conversation.sendText('gm2!')

    await expect.poll(() => messages.length, { timeout: 15_000 }).toBe(4)
    await stream.endAndWait()
    expect(errors).toEqual([])
    expectStreamedMessages(
      messages,
      [
        [message1, 'gm!'],
        [message2, 'gm2!'],
      ],
      [group1.id(), group2.id()]
    )
  })

  it('should only stream dm messages', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const user3 = createUser()
    const user4 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const client3 = await createRegisteredClient(user3)
    const client4 = await createRegisteredClient(user4)
    await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client1.conversations().createGroupByIdentity([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const dm = await client1.conversations().createDmByIdentity({
      identifier: user4.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    await sleep(2000)

    let messages: Message[] = []
    const errors: Error[] = []
    const stream = await client1.conversations().streamAllMessages(
      (err, message) => {
        if (err) errors.push(err)
        if (message) messages.push(message)
      },
      () => {
        console.log('closed')
      },
      ConversationType.Dm
    )

    const groups2 = client2.conversations()
    await groups2.sync()
    const groupsList2 = groups2.list()

    const groups3 = client3.conversations()
    await groups3.sync()
    const groupsList3 = groups3.list()

    const groups4 = client4.conversations()
    await groups4.sync()
    const groupsList4 = groups4.list()

    await groupsList2[0].conversation.sendText('gm!')
    await groupsList3[0].conversation.sendText('gm2!')
    const message3 = await groupsList4[0].conversation.sendText('gm3!')

    await expect.poll(() => messages.length, { timeout: 15_000 }).toBe(2)
    await stream.endAndWait()
    expect(errors).toEqual([])
    expectStreamedMessages(messages, [[message3, 'gm3!']], [dm.id()])
  })

  it('stream should process dm messages from new installations without sync', async () => {
    const agent = createUser()
    const user = createUser()
    const agent_client = await createRegisteredClient(agent)
    const user_client_a = await createRegisteredClient(user)

    const dm = await user_client_a.conversations().createDmByIdentity({
      identifier: agent.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    let messages: Message[] = []
    const errors: Error[] = []
    const stream = await agent_client.conversations().streamAllMessages(
      (err, message) => {
        if (err) errors.push(err)
        if (message) messages.push(message)
      },
      () => {
        console.log('closed')
      },
      ConversationType.Dm
    )
    // Client A send a message to the dm with the Agent
    const client_a_groups = user_client_a.conversations()
    // await client_a_groups.sync()
    const client_a_conversations = client_a_groups.list()
    expect(client_a_conversations.length).toBe(1)
    const firstMessage =
      await client_a_conversations[0].conversation.sendText('gm!')

    // confirm the agent received the message
    await expect.poll(() => messages.length, { timeout: 15_000 }).toBe(2)
    expectStreamedMessages(messages, [[firstMessage, 'gm!']], [dm.id()])

    // User introduce Client B
    user.uuid = v4()
    const user_client_b = await createRegisteredClient(user)

    // Client B Creates a DM with the Agent
    const secondDm = await user_client_b.conversations().createDmByIdentity({
      identifier: agent.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    const client_b_groups = user_client_b.conversations()
    await client_b_groups.sync()
    const client_b_conversations = client_b_groups.list()
    expect(client_b_conversations.length).toBe(1)
    const secondMessage =
      await client_b_conversations[0].conversation.sendText('b')

    // confirm the agent received the second message
    await expect
      .poll(
        () =>
          messages
            .filter((message) => message.kind === GroupMessageKind.Application)
            .map((message) => message.id),
        { timeout: 15_000 }
      )
      .toEqual([firstMessage, secondMessage])
    await stream.endAndWait()
    expect(errors).toEqual([])
    const history = agent_client
      .conversations()
      .messageHistorySnapshot(100)
      .messages.map((entry) => entry.message)
    expect(messages.map((message) => message.id)).toEqual(
      history.map((message) => message.id)
    )
    const membership = history.filter(
      (message) => message.kind === GroupMessageKind.MembershipChange
    )
    expect(
      [...new Set(membership.map((message) => message.convoId))].sort()
    ).toEqual([...new Set([dm.id(), secondDm.id()])].sort())
    expectStreamedMessages(
      messages,
      [
        [firstMessage, 'gm!'],
        [secondMessage, 'b'],
      ],
      membership.map((message) => message.convoId)
    )
  })

  it('should get hmac keys', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    await createRegisteredClient(user2)
    const group = await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const dm = await client1.conversations().createDmByIdentity({
      identifier: user2.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
    const hmacKeys = client1.conversations().hmacKeys()
    expect(hmacKeys).toBeDefined()
    const keys = Object.keys(hmacKeys)
    expect(keys.length).toBe(2)
    expect(keys).toContain(group.id())
    expect(keys).toContain(dm.id())
    for (const values of Object.values(hmacKeys)) {
      expect(values.length).toBe(3)
      for (const value of values) {
        expect(value.key).toBeDefined()
        expect(value.key.length).toBe(42)
        expect(value.epoch).toBeDefined()
        expect(typeof value.epoch).toBe('bigint')
      }
    }
  })

  it('should sync groups across installations', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    user.uuid = v4()
    const client2 = await createRegisteredClient(user)
    const user2 = createUser()
    await createRegisteredClient(user2)

    const group = await client.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client2.conversations().sync()
    const convos = client2.conversations().list()
    expect(convos.length).toBe(1)
    expect(convos[0].conversation.id()).toBe(group.id())

    const group2 = await client.conversations().createDmByIdentity({
      identifier: user2.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
    await client2.conversations().sync()
    const convos2 = client2.conversations().list()
    expect(convos2.length).toBe(2)
    const convos2Ids = convos2.map((c) => c.conversation.id())
    expect(convos2Ids).toContain(group2.id())
    expect(convos2Ids).toContain(group.id())
  })

  it('should create initial group updated messages for added members', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const user3 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    user2.uuid = v4()
    const client2_2 = await createRegisteredClient(user2)
    const client3 = await createRegisteredClient(user3)

    const group1 = await client1
      .conversations()
      .createGroup([client2.inboxId(), client3.inboxId()])
    // Install the first Welcome before removal. An absent group can instead
    // join from the later Welcome when both are pending.
    for (const client of [client2, client2_2, client3]) {
      await client.conversations().sync()
      const joined = client.conversations().getConversationById(group1.id())
      const initialMessages = await joined.listMessages()
      expect(initialMessages).toHaveLength(1)
      expect(initialMessages[0].content.type).toEqual(contentTypeGroupUpdated())
    }
    const firstMessage = await group1.sendText('gm1')
    await group1.removeMembers([client2.inboxId()])
    const excludedMessage = await group1.sendText('gm2')
    await group1.addMembers([client2.inboxId()])
    const lastMessage = await group1.sendText('gm3')

    const messages1 = await group1.listMessages()
    expect(messages1.length).toBe(6)

    await client2.conversations().sync()
    const group2 = client2.conversations().getConversationById(group1.id())
    await group2.sync()
    const messages2 = await group2.listMessages()
    expectStreamedMessages(
      messages2,
      [
        [firstMessage, 'gm1'],
        [lastMessage, 'gm3'],
      ],
      [group1.id(), group1.id(), group1.id()]
    )
    expect(messages2.map((message) => message.id)).not.toContain(
      excludedMessage
    )
    expect(messages2.map((message) => message.content.type)).toEqual([
      contentTypeGroupUpdated(),
      contentTypeText(),
      contentTypeGroupUpdated(),
      contentTypeGroupUpdated(),
      contentTypeText(),
    ])

    await client3.conversations().sync()
    const group3 = client3.conversations().getConversationById(group1.id())
    await group3.sync()
    const messages3 = await group3.listMessages()
    expect(messages3.length).toBe(6)
    expect(messages3[0].content.type).toEqual(contentTypeGroupUpdated())
    expect(messages3[1].content.type).toEqual(contentTypeText())
    expect(messages3[2].content.type).toEqual(contentTypeGroupUpdated())
    expect(messages3[3].content.type).toEqual(contentTypeText())
    expect(messages3[4].content.type).toEqual(contentTypeGroupUpdated())
    expect(messages3[5].content.type).toEqual(contentTypeText())

    await client2_2.conversations().sync()
    const group4 = client2_2.conversations().getConversationById(group1.id())
    await group4.sync()
    const messages4 = await group4.listMessages()
    expectStreamedMessages(
      messages4,
      [
        [firstMessage, 'gm1'],
        [lastMessage, 'gm3'],
      ],
      [group1.id(), group1.id(), group1.id()]
    )
    expect(messages4.map((message) => message.id)).not.toContain(
      excludedMessage
    )
    expect(messages4.map((message) => message.content.type)).toEqual([
      contentTypeGroupUpdated(),
      contentTypeText(),
      contentTypeGroupUpdated(),
      contentTypeGroupUpdated(),
      contentTypeText(),
    ])
  })

  it('should stream deleted messages', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)

    // Create a group
    const group = await client1.conversations().createGroupByIdentity([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])

    // Send a message
    const messageId = await group.sendText('Hello, world!')

    // Set up the deletion stream
    const deletedMessages: DecodedMessage[] = []
    const stream = await client1
      .conversations()
      .streamMessageDeletions((err, message) => {
        if (message) {
          deletedMessages.push(message)
        }
      })

    // Wait for stream to be ready
    await sleep(500)

    // Delete the message
    const deletedCount = client1.conversations().deleteMessageById(messageId)
    expect(deletedCount).toBe(1)

    // Wait for stream to receive the deleted message
    await sleep(1000)

    // Verify the stream received the deleted message with full details
    expect(deletedMessages.length).toBe(1)
    expect(deletedMessages[0].id).toBe(messageId)
    expect(deletedMessages[0].senderInboxId).toBe(client1.inboxId())
    expect(deletedMessages[0].conversationId).toBe(group.id())

    stream.end()
  })
})
