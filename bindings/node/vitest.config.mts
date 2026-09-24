/// <reference types="vitest" />
import { defineConfig, mergeConfig } from 'vite'
import tsconfigPaths from 'vite-tsconfig-paths'
import { defineConfig as defineVitestConfig } from 'vitest/config'

// https://vitejs.dev/config/
const viteConfig = defineConfig({
  plugins: [tsconfigPaths()],
})

const vitestConfig = defineVitestConfig({
  test: {
    globalSetup: ['./vitest.setup.mts'],
    testTimeout: 30000,
    // Opt out of the Rust db-lock panic; transient contention is retried, not
    // fatal, and panicking crashes fork workers. See xmtp/libxmtp#3765.
    env: { XMTP_NO_PANIC_ON_DB_LOCK: 'true' },
  },
})

export default mergeConfig(viteConfig, vitestConfig)
