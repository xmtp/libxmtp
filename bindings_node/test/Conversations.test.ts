import { describe, expect, it } from 'vitest'
import {
  createRegisteredClient,
  createUser,
  encodeTextMessage,
} from '@test/helpers'
import {
  ConsentState,
  Conversation,
  GroupPermissionsOptions,
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
    expect(client.conversations().listDms().length).toBe(0)
    expect(client.conversations().listGroups().length).toBe(0)
  })

  it('should create a group chat', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1
      .conversations()
      .createGroup([user2.account.address])
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

    expect(client1.conversations().listDms().length).toBe(0)
    expect(client1.conversations().listGroups().length).toBe(1)

    expect(client2.conversations().list().length).toBe(0)

    await client2.conversations().sync()

    const groups2 = client2.conversations().list()
    expect(groups2.length).toBe(1)
    expect(groups2[0].conversation.id()).toBe(group.id())

    expect(client2.conversations().listDms().length).toBe(0)
    expect(client2.conversations().listGroups().length).toBe(1)
  })

  it('should create a group with custom permissions', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1
      .conversations()
      .createGroup([user2.account.address], {
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
      })
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
    const group = await client1
      .conversations()
      .createGroup([user2.account.address])

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
    const group = await client1.conversations().createDm(user2.account.address)
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

    expect(client1.conversations().listDms().length).toBe(1)
    expect(client1.conversations().listGroups().length).toBe(0)

    expect(client2.conversations().list().length).toBe(0)

    await client2.conversations().sync()

    const groups2 = client2.conversations().list()
    expect(groups2.length).toBe(1)
    expect(groups2[0].conversation.id()).toBe(group.id())
    expect(groups2[0].conversation.dmPeerInboxId()).toBe(client1.inboxId())

    expect(client2.conversations().listDms().length).toBe(1)
    expect(client2.conversations().listGroups().length).toBe(0)

    const dm1 = client1.conversations().findDmByTargetInboxId(client2.inboxId())
    expect(dm1).toBeDefined()
    expect(dm1!.id).toBe(group.id)

    const dm2 = client2.conversations().findDmByTargetInboxId(client1.inboxId())
    expect(dm2).toBeDefined()
    expect(dm2!.id).toBe(group.id)
  })

  it('should find a group by ID', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1
      .conversations()
      .createGroup([user2.account.address])
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
    const group = await client1
      .conversations()
      .createGroup([user2.account.address])
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
    const groupWithName = await client1
      .conversations()
      .createGroup([user2.account.address], {
        groupName: 'foo',
      })
    expect(groupWithName).toBeDefined()
    expect(groupWithName.groupName()).toBe('foo')
    expect(groupWithName.groupImageUrlSquare()).toBe('')

    const groupWithImageUrl = await client1
      .conversations()
      .createGroup([user3.account.address], {
        groupImageUrlSquare: 'https://foo/bar.png',
      })
    expect(groupWithImageUrl).toBeDefined()
    expect(groupWithImageUrl.groupName()).toBe('')
    expect(groupWithImageUrl.groupImageUrlSquare()).toBe('https://foo/bar.png')

    const groupWithNameAndImageUrl = await client1
      .conversations()
      .createGroup([user4.account.address], {
        groupImageUrlSquare: 'https://foo/bar.png',
        groupName: 'foo',
      })
    expect(groupWithNameAndImageUrl).toBeDefined()
    expect(groupWithNameAndImageUrl.groupName()).toBe('foo')
    expect(groupWithNameAndImageUrl.groupImageUrlSquare()).toBe(
      'https://foo/bar.png'
    )

    const groupWithPermissions = await client1
      .conversations()
      .createGroup([user4.account.address], {
        permissions: GroupPermissionsOptions.AdminOnly,
      })
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

    const groupWithDescription = await client1
      .conversations()
      .createGroup([user2.account.address], {
        groupDescription: 'foo',
      })
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
    const group = await client1
      .conversations()
      .createGroup([user2.account.address])

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
    const stream = client3.conversations().stream((err, convo) => {
      groups.push(convo!)
    })
    const group1 = await client1
      .conversations()
      .createGroup([user3.account.address])
    const group2 = await client2
      .conversations()
      .createGroup([user3.account.address])
    const group3 = await client4.conversations().createDm(user3.account.address)

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
    const stream = client3.conversations().streamGroups((err, convo) => {
      groups.push(convo!)
    })
    const group3 = await client4.conversations().createDm(user3.account.address)
    const group1 = await client1
      .conversations()
      .createGroup([user3.account.address])
    const group2 = await client2
      .conversations()
      .createGroup([user3.account.address])

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
    const stream = client3.conversations().streamDms((err, convo) => {
      groups.push(convo!)
    })
    const group1 = await client1
      .conversations()
      .createGroup([user3.account.address])
    const group2 = await client2
      .conversations()
      .createGroup([user3.account.address])
    const group3 = await client4.conversations().createDm(user3.account.address)

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
    await client1.conversations().createGroup([user2.account.address])
    await client1.conversations().createGroup([user3.account.address])
    await client1.conversations().createDm(user4.account.address)

    const messages: Message[] = []
    const stream = client1.conversations().streamAllMessages((err, message) => {
      messages.push(message!)
    })

    const messages2: Message[] = []
    const stream2 = client2
      .conversations()
      .streamAllMessages((err, message) => {
        messages2.push(message!)
      })

    const messages3: Message[] = []
    const stream3 = client3
      .conversations()
      .streamAllMessages((err, message) => {
        messages3.push(message!)
      })

    const messages4: Message[] = []
    const stream4 = client4
      .conversations()
      .streamAllMessages((err, message) => {
        messages4.push(message!)
      })

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
    await client1.conversations().createGroup([user2.account.address])
    await client1.conversations().createGroup([user3.account.address])
    await client1.conversations().createDm(user4.account.address)

    let messages: Message[] = []
    const stream = client1
      .conversations()
      .streamAllGroupMessages((err, message) => {
        messages.push(message!)
      })

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
    await client1.conversations().createGroup([user2.account.address])
    await client1.conversations().createGroup([user3.account.address])
    await client1.conversations().createDm(user4.account.address)

    let messages: Message[] = []
    const stream = client1
      .conversations()
      .streamAllDmMessages((err, message) => {
        messages.push(message!)
      })

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

  it('should manage group consent state', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const group = await client1
      .conversations()
      .createGroup([user2.account.address])
    expect(group).toBeDefined()

    await client2.conversations().sync()
    const group2 = client2.conversations().findGroupById(group.id())
    expect(group2).toBeDefined()
    expect(group2.consentState()).toBe(ConsentState.Unknown)
    await group2.send(encodeTextMessage('gm!'))
    expect(group2.consentState()).toBe(ConsentState.Allowed)
  })

  it('should update group metadata in empty group', async () => {
    const user1 = createUser()
    const client1 = await createRegisteredClient(user1)

    // Create empty group with admin-only permissions
    const group = await client1.conversations().createGroup([], {
      permissions: GroupPermissionsOptions.AdminOnly,
    })
    expect(group).toBeDefined()

    // Update group name without syncing first
    await group.updateGroupName('New Group Name 1')
    expect(group.groupName()).toBe('New Group Name 1')

    // Verify name persists after sync
    await group.sync()
    expect(group.groupName()).toBe('New Group Name 1')

    // Create another empty group
    const soloGroup = await client1.conversations().createGroup([], {
      permissions: GroupPermissionsOptions.AdminOnly,
    })
    expect(soloGroup).toBeDefined()

    // Update and verify name
    await soloGroup.updateGroupName('New Group Name 2')
    expect(soloGroup.groupName()).toBe('New Group Name 2')

    // Verify name persists after sync
    await soloGroup.sync()
    expect(soloGroup.groupName()).toBe('New Group Name 2')
  })
})
