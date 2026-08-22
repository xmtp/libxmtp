import { randomBytes } from 'node:crypto'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import {
  createLocalBackend,
  createRegisteredClient,
  createUser,
  type User,
} from '@test/helpers'
import {
  checkDatabaseIntegrity,
  createClientWithBackend,
  generateInboxId,
  IdentifierKind,
  IntegrityCheckLevel,
  LogLevel,
  SyncWorkerMode,
} from '../dist'

const __dirname = dirname(fileURLToPath(import.meta.url))

// Mirrors `helpers.ts`'s internal `dbPath` computation (`createClient` does
// not expose it) so `checkDatabaseIntegrity` can be pointed at the same file
// after a client using that path has been closed.
const dbPathFor = (user: User) => join(__dirname, `${user.uuid}.db3`)

// `helpers.ts`'s `createClient` doesn't thread an `encryptionKey` through
// `DbOptions`, so the wrong-key scenario builds a client directly. Identity
// registration is skipped: the assertion is about decrypting the file, not
// about what is stored in it.
const createEncryptedClient = async (user: User, encryptionKey: Uint8Array) => {
  const identifier = {
    identifier: user.account.address,
    identifierKind: IdentifierKind.Ethereum,
  }
  return createClientWithBackend(
    await createLocalBackend(),
    { dbPath: dbPathFor(user), encryptionKey },
    generateInboxId(identifier),
    identifier,
    SyncWorkerMode.Disabled,
    { level: LogLevel.Error },
    undefined
  )
}

describe('integrity', () => {
  it('reports ok for a healthy client db, live and by path', async () => {
    const user = createUser()
    const client = await createRegisteredClient(user)

    const live = await client.dbIntegrityCheck(IntegrityCheckLevel.Full)
    expect(live.outcome).toBe('ok')
    expect(live.findings).toEqual([])

    await client.close()
    const byPath = await checkDatabaseIntegrity(
      dbPathFor(user),
      undefined,
      IntegrityCheckLevel.Quick
    )
    expect(byPath.outcome).toBe('ok')
  })

  it('reports unreadable for a wrong key', async () => {
    const user = createUser()
    const encryptionKey = new Uint8Array(randomBytes(32))
    const client = await createEncryptedClient(user, encryptionKey)
    await client.close()
    const wrongKey = new Uint8Array(32).fill(7)
    const res = await checkDatabaseIntegrity(dbPathFor(user), wrongKey)
    expect(res.outcome).toBe('unreadable')
  })
})
