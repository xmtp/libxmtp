import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createWalletClient, http, toBytes } from 'viem'
import { generatePrivateKey, privateKeyToAccount } from 'viem/accounts'
import { sepolia } from 'viem/chains'
import {
  createClient as create,
  generateInboxId,
  getInboxIdForAddress,
} from '../dist/index'

const __dirname = dirname(fileURLToPath(import.meta.url))
export const TEST_API_URL = 'http://localhost:5556'

export const createUser = () => {
  const key = generatePrivateKey()
  const account = privateKeyToAccount(key)
  return {
    key,
    account,
    wallet: createWalletClient({
      account,
      chain: sepolia,
      transport: http(),
    }),
  }
}

export type User = ReturnType<typeof createUser>

export const createClient = async (user: User) => {
  const dbPath = join(__dirname, `${user.account.address}.db3`)
  const inboxId =
    (await getInboxIdForAddress(TEST_API_URL, false, user.account.address)) ||
    generateInboxId(user.account.address)
  return create(TEST_API_URL, false, dbPath, inboxId, user.account.address)
}

export const createRegisteredClient = async (user: User) => {
  const client = await createClient(user)
  if (!client.isRegistered()) {
    const signatureText = client.signatureText()
    if (signatureText) {
      const signature = await user.wallet.signMessage({
        message: signatureText,
      })
      client.addEcdsaSignature(toBytes(signature))
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
