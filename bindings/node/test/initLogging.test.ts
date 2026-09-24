import { describe, expect, it } from 'vitest'
import { initLogging, LogLevel } from '../dist/index'

// Regression test: a sync `initLogging` panicked building the OTLP tonic exporter with no Tokio reactor; the async binding must resolve instead.
describe('initLogging', () => {
  it('does not panic without an OTLP endpoint', async () => {
    await expect(initLogging({ level: LogLevel.Info })).resolves.toBeUndefined()
  })

  it('does not panic when an OTLP endpoint is configured (the boot-panic regression)', async () => {
    // A non-listening endpoint is fine: the build panics for lack of a reactor regardless, and post-build export errors are background, not rejections.
    await expect(
      initLogging({
        level: LogLevel.Info,
        otelEndpoint: 'http://127.0.0.1:4319',
        resourceAttributes: { 'service.name': 'init-logging-test' },
      })
    ).resolves.toBeUndefined()
  })

  it('is idempotent — a second call is a no-op and still does not throw', async () => {
    // Process-global, first-call-wins: the second call short-circuits without a second exporter build.
    await expect(
      initLogging({ level: LogLevel.Debug })
    ).resolves.toBeUndefined()
  })
})
