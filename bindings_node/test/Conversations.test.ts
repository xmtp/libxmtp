import {
  ContentTypeGroupUpdated,
  GroupUpdatedCodec,
} from '@xmtp/content-type-group-updated'
import { ContentTypeText, TextCodec } from '@xmtp/content-type-text'
import { v4 } from 'uuid'
import { describe, expect, it } from 'vitest'
import {
  createRegisteredClient,
  createUser,
  encodeTextMessage,
} from '@test/helpers'
import {
  ConsentState,
  Conversation,
  ConversationType,
  GroupPermissionsOptions,
  IdentifierKind,
  Message,
  MetadataField,
  PermissionPolicy,
  PermissionUpdateType,
} from '../dist'

const SLEEP_MS = 100
const sleep = () => new Promise((resolve) => setTimeout(resolve, SLEEP_MS))

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
    const group = await client1.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    expect(group).toBeDefined()
    expect(group.id()).toBeDefined()
    expect(group.createdAtNs()).toBeTypeOf('number')
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
      updateGroupNamePolicy: 0,
      updateGroupDescriptionPolicy: 0,
      updateGroupImageUrlSquarePolicy: 0,
      updateMessageDisappearingPolicy: 2,
    })
    expect(group.addedByInboxId()).toBe(client1.inboxId())
    expect((await group.findMessages()).length).toBe(1)
    const members = await group.listMembers()
    expect(members.length).toBe(2)
    const memberInboxIds = members.map((member) => member.inboxId)
    expect(memberInboxIds).toContain(client1.inboxId())
    expect(memberInboxIds).toContain(client2.inboxId())
    expect((await group.groupMetadata()).conversationType()).toBe('group')
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
    const group = await client1.conversations().createGroup(
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
    const group = await client1.conversations().createGroup([
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
    const group = await client1.conversations().createDm({
      identifier: user2.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
    expect(group).toBeDefined()
    expect(group.id()).toBeDefined()
    expect(group.createdAtNs()).toBeTypeOf('number')
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
      updateGroupDescriptionPolicy: 0,
      updateGroupImageUrlSquarePolicy: 0,
      updateGroupNamePolicy: 0,
      updateMessageDisappearingPolicy: 0,
    })
    expect(group.addedByInboxId()).toBe(client1.inboxId())
    expect((await group.findMessages()).length).toBe(1)
    const members = await group.listMembers()
    expect(members.length).toBe(2)
    const memberInboxIds = members.map((member) => member.inboxId)
    expect(memberInboxIds).toContain(client1.inboxId())
    expect(memberInboxIds).toContain(client2.inboxId())
    expect((await group.groupMetadata()).conversationType()).toBe('dm')
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

    const dm1 = client1.conversations().findDmByTargetInboxId(client2.inboxId())
    expect(dm1).toBeDefined()
    expect(dm1!.id()).toBe(group.id())

    const dm2 = client2.conversations().findDmByTargetInboxId(client1.inboxId())
    expect(dm2).toBeDefined()
    expect(dm2!.id()).toBe(group.id())
  })

  it('should find a group by ID', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    expect(group).toBeDefined()
    expect(group.id()).toBeDefined()
    const foundGroup = client1.conversations().findGroupById(group.id())
    expect(foundGroup).toBeDefined()
    expect(foundGroup!.id()).toBe(group.id())
  })

  it('should find a message by ID', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    await createRegisteredClient(user2)
    const group = await client1.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const messageId = await group.send(encodeTextMessage('gm!'))
    expect(messageId).toBeDefined()

    const message = client1.conversations().findMessageById(messageId)
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
    const groupWithName = await client1.conversations().createGroup(
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

    const groupWithImageUrl = await client1.conversations().createGroup(
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

    const groupWithNameAndImageUrl = await client1.conversations().createGroup(
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

    const groupWithPermissions = await client1.conversations().createGroup(
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
      updateGroupNamePolicy: 2,
      updateGroupDescriptionPolicy: 2,
      updateGroupImageUrlSquarePolicy: 2,
      updateMessageDisappearingPolicy: 2,
    })

    const groupWithDescription = await client1.conversations().createGroup(
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
    const group = await client1.conversations().createGroup([
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
    const stream = client3.conversations().stream(
      (err, convo) => {
        groups.push(convo!)
      },
      () => {
        console.log('closed')
      }
    )
    const group1 = await client1.conversations().createGroup([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group2 = await client2.conversations().createGroup([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group3 = await client4.conversations().createDm({
      identifier: user3.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    await sleep()

    stream.end()
    expect(groups.length).toBe(3)
    expect(groups).toEqual([group1, group2, group3])
  })

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
    const stream = client3.conversations().stream(
      (err, convo) => {
        groups.push(convo!)
      },
      () => {
        console.log('clossed')
      },
      ConversationType.Group
    )
    const group3 = await client4.conversations().createDm({
      identifier: user3.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
    const group1 = await client1.conversations().createGroup([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group2 = await client2.conversations().createGroup([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])

    await sleep()

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
    const stream = client3.conversations().stream(
      (err, convo) => {
        groups.push(convo!)
      },
      () => {
        console.log('closed')
      },
      ConversationType.Dm
    )
    const group1 = await client1.conversations().createGroup([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group2 = await client2.conversations().createGroup([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const group3 = await client4.conversations().createDm({
      identifier: user3.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    await sleep()

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
    await client1.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client1.conversations().createGroup([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client1.conversations().createDm({
      identifier: user4.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    const messages: Message[] = []
    const stream = client1.conversations().streamAllMessages(
      (err, message) => {
        messages.push(message!)
      },
      () => {
        console.log('closed')
      },
      undefined,
      [ConsentState.Allowed, ConsentState.Unknown]
    )

    const messages2: Message[] = []
    const stream2 = client2.conversations().streamAllMessages(
      (err, message) => {
        messages2.push(message!)
      },
      () => {
        console.log('closed')
      },
      undefined,
      [ConsentState.Allowed, ConsentState.Unknown]
    )

    const messages3: Message[] = []
    const stream3 = client3.conversations().streamAllMessages(
      (err, message) => {
        messages3.push(message!)
      },
      () => {
        console.log('closed')
      },
      undefined,
      [ConsentState.Allowed, ConsentState.Unknown]
    )

    const messages4: Message[] = []
    const stream4 = client4.conversations().streamAllMessages(
      (err, message) => {
        messages4.push(message!)
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

    const message1 = await groupsList2[0].conversation.send(
      encodeTextMessage('gm!')
    )
    const message2 = await groupsList3[0].conversation.send(
      encodeTextMessage('gm2!')
    )
    const message3 = await groupsList4[0].conversation.send(
      encodeTextMessage('gm3!')
    )

    await sleep()

    stream.end()
    stream2.end()
    stream3.end()
    stream4.end()
    expect(messages.length).toBe(3)
    expect(messages.map((m) => m.id)).toEqual([message1, message2, message3])
    expect(messages2.length).toBe(1)
    expect(messages2.map((m) => m.id)).toEqual([message1])
    expect(messages3.length).toBe(1)
    expect(messages3.map((m) => m.id)).toEqual([message2])
    expect(messages4.length).toBe(1)
    expect(messages4.map((m) => m.id)).toEqual([message3])
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
    await client1.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client1.conversations().createGroup([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client1.conversations().createDm({
      identifier: user4.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    let messages: Message[] = []
    const stream = client1.conversations().streamAllMessages(
      (err, message) => {
        messages.push(message!)
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

    await groupsList4[0].conversation.send(encodeTextMessage('gm3!'))
    const message1 = await groupsList2[0].conversation.send(
      encodeTextMessage('gm!')
    )
    const message2 = await groupsList3[0].conversation.send(
      encodeTextMessage('gm2!')
    )

    await sleep()

    stream.end()
    expect(messages.length).toBe(2)
    expect(messages.map((m) => m.id)).toEqual([message1, message2])
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
    await client1.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client1.conversations().createGroup([
      {
        identifier: user3.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client1.conversations().createDm({
      identifier: user4.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })

    let messages: Message[] = []
    const stream = client1.conversations().streamAllMessages(
      (err, message) => {
        messages.push(message!)
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

    await groupsList2[0].conversation.send(encodeTextMessage('gm!'))
    await groupsList3[0].conversation.send(encodeTextMessage('gm2!'))
    const message3 = await groupsList4[0].conversation.send(
      encodeTextMessage('gm3!')
    )

    await sleep()

    stream.end()
    expect(messages.length).toBe(1)
    expect(messages.map((m) => m.id)).toEqual([message3])
  })

  it('should get hmac keys', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    await createRegisteredClient(user2)
    const group = await client1.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    const dm = await client1.conversations().createDm({
      identifier: user2.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
    const hmacKeys = client1.conversations().getHmacKeys()
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

    const group = await client.conversations().createGroup([
      {
        identifier: user2.account.address,
        identifierKind: IdentifierKind.Ethereum,
      },
    ])
    await client2.conversations().sync()
    const convos = client2.conversations().list()
    expect(convos.length).toBe(1)
    expect(convos[0].conversation.id()).toBe(group.id())

    const group2 = await client.conversations().createDm({
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
    const groupUpdatedCodec = new GroupUpdatedCodec()
    const textCodec = new TextCodec()
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
      .createGroupByInboxId([client2.inboxId(), client3.inboxId()])
    await group1.send(encodeTextMessage('gm1'))
    await group1.removeMembersByInboxId([client2.inboxId()])
    await group1.send(encodeTextMessage('gm2'))
    await group1.addMembersByInboxId([client2.inboxId()])
    await group1.send(encodeTextMessage('gm3'))

    const messages1 = await group1.findMessages()
    expect(messages1.length).toBe(6)

    await client2.conversations().sync()
    const group2 = client2.conversations().findGroupById(group1.id())
    await group2.sync()
    const messages2 = await group2.findMessages()
    expect(messages2.length).toBe(3)
    expect(messages2[0].content.type).toEqual(ContentTypeGroupUpdated)
    expect(messages2[1].content.type).toEqual(ContentTypeGroupUpdated)
    expect(messages2[2].content.type).toEqual(ContentTypeText)

    await client3.conversations().sync()
    const group3 = client3.conversations().findGroupById(group1.id())
    await group3.sync()
    const messages3 = await group3.findMessages()
    expect(messages3.length).toBe(6)
    expect(messages3[0].content.type).toEqual(ContentTypeGroupUpdated)
    expect(messages3[1].content.type).toEqual(ContentTypeText)
    expect(messages3[2].content.type).toEqual(ContentTypeGroupUpdated)
    expect(messages3[3].content.type).toEqual(ContentTypeText)
    expect(messages3[4].content.type).toEqual(ContentTypeGroupUpdated)
    expect(messages3[5].content.type).toEqual(ContentTypeText)

    await client2_2.conversations().sync()
    const group4 = client2_2.conversations().findGroupById(group1.id())
    await group4.sync()
    const messages4 = await group4.findMessages()
    expect(messages4.length).toBe(3)
    expect(messages4[0].content.type).toEqual(ContentTypeGroupUpdated)
    expect(messages4[1].content.type).toEqual(ContentTypeGroupUpdated)
    expect(messages4[2].content.type).toEqual(ContentTypeText)
  })
})
