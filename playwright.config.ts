import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests/ui",
  fullyParallel: false,
  reporter: "list",
  use: {
    baseURL: "http://127.0.0.1:1420",
    channel: process.env.CI ? undefined : "chrome",
    headless: true,
  },
  webServer: {
    command: "npm run dev",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: true,
  },
});
