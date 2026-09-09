import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  testMatch: "**/*.spec.mjs",
  fullyParallel: true,
  workers: 2,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: process.env.DOCS_BASE_URL ?? "http://127.0.0.1:4322",
    trace: "retain-on-failure",
  },
  webServer: process.env.DOCS_BASE_URL
    ? undefined
    : {
        command: "node scripts/check-serve.mjs",
        port: 4322,
        reuseExistingServer: false,
      },
  projects: [
    {
      name: "desktop-light",
      use: { viewport: { width: 1440, height: 900 }, colorScheme: "light" },
    },
    {
      name: "desktop-dark",
      use: { viewport: { width: 1440, height: 900 }, colorScheme: "dark" },
    },
    {
      name: "mobile-light",
      use: { viewport: { width: 390, height: 844 }, colorScheme: "light" },
    },
    {
      name: "mobile-dark",
      use: { viewport: { width: 390, height: 844 }, colorScheme: "dark" },
    },
  ],
});
