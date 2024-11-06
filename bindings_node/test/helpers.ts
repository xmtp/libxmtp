import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { v4 } from 'uuid'
import { createWalletClient, http, toBytes } from 'viem'
import { toCoinbaseSmartAccount } from 'viem/account-abstraction'
import { generatePrivateKey, privateKeyToAccount } from 'viem/accounts'
import { base } from 'viem/chains'
import {
  createClient as create,
  generateInboxId,
  getInboxIdForAddress,
  NapiSignatureRequestType,
} from '../dist/index'

const __dirname = dirname(fileURLToPath(import.meta.url))
export const TEST_API_URL = 'http://localhost:5556'

export const createUser = () => {
  const key = generatePrivateKey()
  const account = privateKeyToAccount(key)
  return {
    isSCW: false,
    key,
    account,
    wallet: createWalletClient({
      account,
      chain: base,
      transport: http(),
    }),
    uuid: v4(),
  }
}

export type User = ReturnType<typeof createUser>

export const createSCWUser = async () => {
  const client = createWalletClient({
    chain: base,
    transport: http(),
  })
  const key = generatePrivateKey()
  const owner = privateKeyToAccount(key)
  const account = await toCoinbaseSmartAccount({
    client,
    owners: [owner],
  })
  return {
    isSCW: true,
    key,
    account,
    wallet: client,
    uuid: v4(),
  }
}

export type SCWUser = Awaited<ReturnType<typeof createSCWUser>>

export const createClient = async (user: User | SCWUser) => {
  const dbPath = join(__dirname, `${user.uuid}.db3`)
  const inboxId =
    (await getInboxIdForAddress(TEST_API_URL, false, user.account.address)) ||
    generateInboxId(user.account.address)
  return create(
    TEST_API_URL,
    false,
    dbPath,
    inboxId,
    user.account.address,
    undefined,
    undefined,
    'error'
  )
}

export const createRegisteredClient = async (user: User | SCWUser) => {
  const client = await createClient(user)
  if (!client.isRegistered()) {
    const signatureText = await client.createInboxSignatureText()
    if (signatureText) {
      const signature = await user.wallet.signMessage({
        message: signatureText,
        account: user.account,
      })
      if (!user.isSCW) {
        await client.addSignature(
          NapiSignatureRequestType.CreateInbox,
          toBytes(signature)
        )
      } else {
        await client.addScwSignature(
          NapiSignatureRequestType.CreateInbox,
          toBytes(signature),
          BigInt(base.id)
        )
      }
    }
    await client.registerIdentity()
  }
  return client
}

export const encodeTextMessage = (text: string) => {
  return {
    type: {
      authorityId: 'xmtp.org',
      typeId: 'text',
      versionMajor: 1,
      versionMinor: 0,
    },
    parameters: {
      encoding: 'UTF-8',
    },
    content: new TextEncoder().encode(text),
  }
}
