import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { v4 } from 'uuid'
import { createWalletClient, http, toBytes } from 'viem'
import { generatePrivateKey, privateKeyToAccount } from 'viem/accounts'
import { sepolia } from 'viem/chains'
import {
  BackendBuilder,
  createClientWithBackend as create,
  createLocalToxicClient,
  generateInboxId,
  getInboxIdByIdentity,
  IdentifierKind,
  LogLevel,
  SyncWorkerMode,
  XmtpEnv,
} from '../dist/index'

const __dirname = dirname(fileURLToPath(import.meta.url))
export const TEST_API_URL = 'http://localhost:5556'
export const GATEWAY_TEST_URL = 'http://localhost:5052'

export const createLocalBackend = async (appVersion?: string) => {
  const builder = new BackendBuilder(XmtpEnv.Local).setApiUrl(TEST_API_URL)
  if (appVersion) {
    builder.setAppVersion(appVersion)
  }
  return builder.build()
}

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
  const backend = await createLocalBackend(appVersion)
  const inboxId =
    (await getInboxIdByIdentity(backend, {
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })) ||
    generateInboxId({
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    })
  return create(
    backend,
    {
      dbPath,
    },
    inboxId,
    {
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    },
    SyncWorkerMode.Disabled,
    { level: LogLevel.Error },
    undefined
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
  const backend = await createLocalBackend()
  const inboxId =
    (await getInboxIdByIdentity(backend, {
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
    SyncWorkerMode.Disabled,
    { level: LogLevel.Debug },
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

export function sleep(ms: number) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms)
  })
}
