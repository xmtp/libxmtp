import { describe, expect, it } from 'vitest'
import { BackendBuilder, NapiTestBuilder, XmtpEnv } from '../dist/index'

describe('BackendBuilder', () => {
  it('should build a Backend with default settings', async () => {
    const backend = await new BackendBuilder(XmtpEnv.Dev).build()
    expect(backend.env).toBe(XmtpEnv.Dev)
    expect(backend.v3Host).toBe('https://grpc.dev.xmtp.network:443')
    expect(backend.gatewayHost).toBeNull()
    expect(backend.appVersion).toBe('')
  })

  it('should build with custom app version', async () => {
    const backend = await new BackendBuilder(XmtpEnv.Dev)
      .setAppVersion('TestApp/1.0')
      .build()
    expect(backend.appVersion).toBe('TestApp/1.0')
  })

  it('should reject double build', async () => {
    const builder = new BackendBuilder(XmtpEnv.Dev)
    await builder.build()
    await expect(builder.build()).rejects.toThrow('already been consumed')
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
