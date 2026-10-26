import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './website/tests',
  outputDir: 'test-results/pages',
  workers: 1,
  fullyParallel: false,
  timeout: 20_000,
  expect: { timeout: 5_000 },
  use: {
    baseURL: process.env.PAGES_TEST_URL || 'http://127.0.0.1:4178/',
    trace: 'retain-on-failure',
    reducedMotion: 'reduce',
  },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 1000 } } },
    { name: 'mobile', use: { ...devices['Pixel 7'], deviceScaleFactor: 1 } },
  ],
  webServer: process.env.PAGES_TEST_URL ? undefined : {
    command: 'python3 -m http.server 4178 --bind 127.0.0.1 --directory dist/pages',
    url: 'http://127.0.0.1:4178',
    reuseExistingServer: false,
    timeout: 10_000,
  },
});
