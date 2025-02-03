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
  },
})

export default mergeConfig(viteConfig, vitestConfig)
