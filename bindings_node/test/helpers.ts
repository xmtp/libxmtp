import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { v4 } from 'uuid'
import { createWalletClient, http, toBytes } from 'viem'
import { generatePrivateKey, privateKeyToAccount } from 'viem/accounts'
import { sepolia } from 'viem/chains'
import {
  createClient as create,
  generateInboxId,
  getInboxIdForIdentifier,
  Identifier,
  IdentifierKind,
  LogLevel,
  SignatureRequestType,
  SyncWorkerMode,
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
    uuid: v4(),
  }
}

export type User = ReturnType<typeof createUser>

export const createClient = async (user: User) => {
  const dbPath = join(__dirname, `${user.uuid}.db3`)
  const inboxId =
    (await getInboxIdForIdentifier(TEST_API_URL, false, {
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })) ||
    generateInboxId({
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
  return create(
    TEST_API_URL,
    false,
    dbPath,
    inboxId,
    {
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    },
    undefined,
    undefined,
    SyncWorkerMode.disabled,
    { level: LogLevel.off }
  )
}

export const createRegisteredClient = async (user: User) => {
  const client = await createClient(user)
  if (!client.isRegistered()) {
    const signatureText = await client.createInboxSignatureText()
    if (signatureText) {
      const signature = await user.wallet.signMessage({
        message: signatureText,
      })
      await client.addEcdsaSignature(
        SignatureRequestType.CreateInbox,
        toBytes(signature)
      )
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

export function sleep(ms: number) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms)
  })
}
