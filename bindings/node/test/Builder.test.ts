import { toBytes } from 'viem'
import { describe, expect, it } from 'vitest'
import {
  AuthCallback,
  AuthHandle,
  BackendBuilder,
  createClientWithBackend,
  generateInboxId,
  getInboxIdByIdentity,
  IdentifierKind,
  NapiTestBuilder,
  SyncWorkerMode,
} from '../dist/index'
import { createUser, TEST_API_URL } from './helpers'

describe('BackendBuilder', () => {
  it('API-client cache key uses backend URL and app version', async () => {
    const first = await new BackendBuilder(TEST_API_URL)
      .setEnv('local')
      .setAppVersion('TestApp/1.0')
      .build()
    const otherEnv = await new BackendBuilder(TEST_API_URL)
      .setEnv('custom')
      .setAppVersion('TestApp/1.0')
      .build()
    expect(first.cacheKey).toBe(`${TEST_API_URL}|TestApp/1.0`)
    expect(otherEnv.cacheKey).toBe(first.cacheKey)
    expect(otherEnv.env).toBe('custom')
    const noVersion = await new BackendBuilder(TEST_API_URL).build()
    expect(noVersion.cacheKey).toBe(`${TEST_API_URL}|`)
    expect(noVersion.cacheKey).not.toBe(first.cacheKey)
    const otherUrl = await new BackendBuilder('http://127.0.0.1:59999').build()
    expect(otherUrl.cacheKey).not.toBe(noVersion.cacheKey)
  })

  it('backend URL is required', async () => {
    // @ts-expect-error The backend URL is required.
    expect(() => new BackendBuilder()).toThrow()
    await expect(new BackendBuilder('').build()).rejects.toThrow()
  })

  it('should build with custom app version', async () => {
    const backend = await new BackendBuilder(TEST_API_URL)
      .setAppVersion('TestApp/1.0')
      .build()
    expect(backend.appVersion).toBe('TestApp/1.0')
  })

  it('should reject double build', async () => {
    const builder = new BackendBuilder(TEST_API_URL)
    await builder.build()
    await expect(builder.build()).rejects.toThrow('already been consumed')
  })
})

describe('Backend authentication', () => {
  it('refreshes credentials once and accepts an auth handle update', async () => {
    let calls = 0
    const callback = new AuthCallback(async () => {
      calls++
      return {
        value: 'Bearer callback',
        expiresAtSeconds: Math.floor(Date.now() / 1000) + 3600,
      }
    })
    const handle = new AuthHandle()
    const builder = new BackendBuilder(TEST_API_URL)
    builder.authCallback(callback)
    builder.authHandle(handle)
    const backend = await builder.build()
    const identifier = {
      identifier: createUser().account.address,
      identifierKind: IdentifierKind.Ethereum,
    }
    const read = () => getInboxIdByIdentity(backend, identifier)
    await Promise.all([read(), read(), read()])
    expect(calls).toBe(1)
    await handle.set({ value: 'Bearer expired', expiresAtSeconds: 0 })
    await Promise.all([read(), read(), read()])
    expect(calls).toBe(2)
    await handle.set({
      value: 'Bearer replacement',
      expiresAtSeconds: Math.floor(Date.now() / 1000) + 3600,
    })
    await read()
    expect(calls).toBe(2)
  })

  it('rejects client creation when the auth callback fails', async () => {
    const callback = new AuthCallback(async () => {
      throw new Error('Auth callback failed')
    })
    const builder = new BackendBuilder(TEST_API_URL)
    builder.authCallback(callback)
    const backend = await builder.build()
    const identifier = {
      identifier: createUser().account.address,
      identifierKind: IdentifierKind.Ethereum,
    }
    await expect(
      createClientWithBackend(
        backend,
        {},
        generateInboxId(identifier),
        identifier
      )
    ).rejects.toThrow('auth callback failed')
  })

  it('rejects a publish from a read-only client', async () => {
    const backend = await new BackendBuilder(TEST_API_URL)
      .setReadonly(true)
      .build()
    const user = createUser()
    const identifier = {
      identifier: user.account.address,
      identifierKind: IdentifierKind.Ethereum,
    }
    const client = await createClientWithBackend(
      backend,
      {},
      generateInboxId(identifier),
      identifier,
      SyncWorkerMode.Disabled
    )
    const request = await client.createInboxSignatureRequest()
    expect(request).toBeDefined()
    const signature = await user.wallet.signMessage({
      message: await request!.signatureText(),
    })
    await request!.addEcdsaSignature(toBytes(signature))
    await expect(client.registerIdentity(request!)).rejects.toThrow(
      'Writes are disabled on this client.'
    )
  })
})

describe('NapiTestBuilder', () => {
  it('should set required fields and apply defaults', () => {
    const b = new NapiTestBuilder('hello')
    expect(b.name).toBe('hello')
    expect(b.flag).toBeNull()
    expect(b.count).toBeNull()
    expect(b.port).toBe(42)
    expect(b.enabled).toBe(true)
  })

  it('should support setter chaining', () => {
    const b = new NapiTestBuilder('chained')
      .setFlag(true)
      .setCount(99)
      .setPort(8080)
      .setEnabled(false)
    expect(b.name).toBe('chained')
    expect(b.flag).toBe(true)
    expect(b.count).toBe(99)
    expect(b.port).toBe(8080)
    expect(b.enabled).toBe(false)
  })

  it('should support partial chaining', () => {
    const b = new NapiTestBuilder('partial').setFlag(false)
    expect(b.name).toBe('partial')
    expect(b.flag).toBe(false)
    expect(b.count).toBeNull()
    expect(b.port).toBe(42)
    expect(b.enabled).toBe(true)
  })

  it('should allow defaults to be overridden', () => {
    const b = new NapiTestBuilder('defaults').setPort(9090).setEnabled(false)
    expect(b.port).toBe(9090)
    expect(b.enabled).toBe(false)
  })
})
