import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { v4 } from 'uuid'
import { createWalletClient, http, toBytes } from 'viem'
import { generatePrivateKey, privateKeyToAccount } from 'viem/accounts'
import { sepolia } from 'viem/chains'
import {
  createClient as create,
  createLocalToxicClient,
  deserializeEncodedContent,
  EncodedContent,
  encodeReaction,
  generateInboxId,
  getInboxIdForIdentifier,
  IdentifierKind,
  LogLevel,
  ReactionAction,
  ReactionSchema,
  serializeEncodedContent,
  SyncWorkerMode,
} from '../dist/index'

const __dirname = dirname(fileURLToPath(import.meta.url))
export const TEST_API_URL = 'http://localhost:5556'
export const GATEWAY_TEST_URL = 'http://localhost:5052'

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

export const createClient = async (user: User, appVersion?: string) => {
  const dbPath = join(__dirname, `${user.uuid}.db3`)
  const inboxId =
    (await getInboxIdForIdentifier(TEST_API_URL, undefined, false, {
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })) ||
    generateInboxId({
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
  return create(
    TEST_API_URL,
    undefined,
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
    { level: LogLevel.error },
    undefined,
    true,
    appVersion ?? null
  )
}

export const createRegisteredClient = async (
  user: User,
  appVersion?: string
) => {
  const client = await createClient(user, appVersion)
  if (!client.isRegistered()) {
    const signatureRequest = await client.createInboxSignatureRequest()
    if (signatureRequest) {
      const signature = await user.wallet.signMessage({
        message: await signatureRequest.signatureText(),
      })
      await signatureRequest.addEcdsaSignature(toBytes(signature))
      await client.registerIdentity(signatureRequest)
    }
  }
  return client
}

export const createToxicClient = async (user: User) => {
  const dbPath = join(__dirname, `${user.uuid}.db3`)
  const inboxId =
    (await getInboxIdForIdentifier(TEST_API_URL, undefined, false, {
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })) ||
    generateInboxId({
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
  return createLocalToxicClient(
    dbPath,
    inboxId,
    {
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    },
    undefined,
    undefined,
    SyncWorkerMode.disabled,
    { level: LogLevel.debug },
    true
  )
}

export const createToxicRegisteredClient = async (user: User) => {
  const toxic_client = await createToxicClient(user)
  const client = toxic_client.client
  if (!client.isRegistered()) {
    const signatureRequest = await client.createInboxSignatureRequest()
    if (signatureRequest) {
      const signature = await user.wallet.signMessage({
        message: await signatureRequest.signatureText(),
      })
      await signatureRequest.addEcdsaSignature(toBytes(signature))
      await client.registerIdentity(signatureRequest)
    }
  }
  return toxic_client
}

export const encodeTextMessage = (text: string): EncodedContent => {
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
    content: new Uint8Array(new TextEncoder().encode(text)),
  }
}

export function sleep(ms: number) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms)
  })
}

export const encodeReactionMessage = (
  reference: string,
  referenceInboxId: string,
  content: string,
  action: ReactionAction = ReactionAction.Added,
  schema: ReactionSchema = ReactionSchema.Unicode
): EncodedContent => {
  // encodeReaction returns the fully encoded EncodedContent as bytes
  // Deserialize it back to an EncodedContent object for send()
  const bytes = encodeReaction({
    reference,
    referenceInboxId,
    action,
    content,
    schema,
  })
  return deserializeEncodedContent(bytes)
}

export const encodeReplyMessage = (referenceId: string, content: string) => {
  // Reply content type using composite codec
  return {
    type: {
      authorityId: 'xmtp.org',
      typeId: 'reply',
      versionMajor: 1,
      versionMinor: 0,
    },
    parameters: {
      reference: referenceId,
    },
    content: serializeEncodedContent(encodeTextMessage(content)),
  }
}
