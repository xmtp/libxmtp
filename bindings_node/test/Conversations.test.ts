import { encode } from 'punycode'
import { describe, expect, it } from 'vitest'
import { AsyncStream } from '@test/AsyncStream'
import {
  createRegisteredClient,
  createUser,
  encodeTextMessage,
} from '@test/helpers'
import { GroupPermissions, NapiGroup, NapiMessage } from '../dist'

describe('Conversations', () => {
  it('should not have initial conversations', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)
    const conversations = client.conversations().list()
    expect((await conversations).length).toBe(0)
  })

  it('should create a new group', async () => {
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
      GroupPermissions.EveryoneIsAdmin
    )
    expect(group.addedByInboxId()).toBe(client1.inboxId())
    expect(group.findMessages().length).toBe(1)
    const members = group.listMembers()
    expect(members.length).toBe(2)
    const memberInboxIds = members.map((member) => member.inboxId)
    expect(memberInboxIds).toContain(client1.inboxId())
    expect(memberInboxIds).toContain(client2.inboxId())
    expect(group.groupMetadata().conversationType()).toBe('group')
    expect(group.groupMetadata().creatorInboxId()).toBe(client1.inboxId())

    const group1 = await client1.conversations().list()
    expect(group1.length).toBe(1)
    expect(group1[0].id).toBe(group.id)

    expect((await client2.conversations().list()).length).toBe(0)

    await client2.conversations().sync()

    const group2 = await client2.conversations().list()
    expect(group2.length).toBe(1)
    expect(group2[0].id).toBe(group.id)
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
        permissions: GroupPermissions.GroupCreatorIsAdmin,
      })
    expect(groupWithPermissions).toBeDefined()
    expect(groupWithPermissions.groupName()).toBe('')
    expect(groupWithPermissions.groupImageUrlSquare()).toBe('')
    expect(groupWithPermissions.groupPermissions().policyType()).toBe(
      GroupPermissions.GroupCreatorIsAdmin
    )

    const groupWithDescription = await client1
      .conversations()
      .createGroup([user2.account.address], {
        groupDescription: 'foo',
      })
    expect(groupWithDescription).toBeDefined()
    expect(groupWithDescription.groupName()).toBe('')
    expect(groupWithDescription.groupImageUrlSquare()).toBe('')
    expect(groupWithDescription.groupDescription()).toBe('foo')

    const groupWithPinnedFrameUrl = await client1
      .conversations()
      .createGroup([user2.account.address], {
        groupPinnedFrameUrl: 'https://frameurl.xyz',
      })
    expect(groupWithPinnedFrameUrl).toBeDefined()
    expect(groupWithPinnedFrameUrl.groupName()).toBe('')
    expect(groupWithPinnedFrameUrl.groupImageUrlSquare()).toBe('')
    expect(groupWithPinnedFrameUrl.groupDescription()).toBe('')
    expect(groupWithPinnedFrameUrl.groupPinnedFrameUrl()).toBe(
      'https://frameurl.xyz'
    )
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

    await group.updateGroupPinnedFrameUrl('https://frameurl.xyz')
    expect(group.groupPinnedFrameUrl()).toBe('https://frameurl.xyz')
  })

  it('should stream new groups', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const user3 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const client3 = await createRegisteredClient(user3)
    const asyncStream = new AsyncStream<NapiGroup>(undefined)
    const stream = client3.conversations().stream(asyncStream.callback)
    const group1 = await client1
      .conversations()
      .createGroup([user3.account.address])
    const group2 = await client2
      .conversations()
      .createGroup([user3.account.address])
    let count = 0
    for await (const convo of asyncStream) {
      count++
      expect(convo).toBeDefined()
      if (count === 1) {
        expect(convo!.id).toBe(group1.id)
      }
      if (count === 2) {
        expect(convo!.id).toBe(group2.id)
        break
      }
    }
    asyncStream.stop()
    stream.end()
  })

  it('should stream all messages', async () => {
    const user1 = createUser()
    const user2 = createUser()
    const user3 = createUser()
    const client1 = await createRegisteredClient(user1)
    const client2 = await createRegisteredClient(user2)
    const client3 = await createRegisteredClient(user3)
    await client1.conversations().createGroup([user2.account.address])
    await client1.conversations().createGroup([user3.account.address])

    const asyncStream = new AsyncStream<NapiMessage>(undefined)
    const stream = client1
      .conversations()
      .streamAllMessages(asyncStream.callback)

    const groups2 = client2.conversations()
    await groups2.sync()
    const groupsList2 = await groups2.list()

    const groups3 = client3.conversations()
    await groups3.sync()
    const groupsList3 = await groups3.list()

    await groupsList2[0].send(encodeTextMessage('gm!'))
    await groupsList3[0].send(encodeTextMessage('gm2!'))

    let count = 0

    for await (const message of asyncStream) {
      count++
      expect(message).toBeDefined()
      if (count === 1) {
        expect(message!.senderInboxId).toBe(client2.inboxId())
      }
      if (count === 2) {
        expect(message!.senderInboxId).toBe(client3.inboxId())
        break
      }
    }
    asyncStream.stop()
    stream.end()
  })
})
