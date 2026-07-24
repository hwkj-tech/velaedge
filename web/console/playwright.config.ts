import { defineConfig, devices } from '@playwright/test';
import { existsSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const consoleDir = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(consoleDir, '../..');
const workDir = process.env.EDGEOPS_E2E_WORK_DIR
  ? resolve(repositoryRoot, process.env.EDGEOPS_E2E_WORK_DIR)
  : resolve(repositoryRoot, 'target/console-e2e');
const httpPort = process.env.EDGEOPS_E2E_HTTP_PORT ?? '18261';
const gatewayPort = process.env.EDGEOPS_E2E_GATEWAY_PORT ?? '19261';
const baseURL = `http://127.0.0.1:${httpPort}`;
const databasePath = resolve(workDir, 'cloud.sqlite');
const macChromePath = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const executablePath =
  process.env.EDGEOPS_E2E_CHROME_PATH ??
  (process.platform === 'darwin' && existsSync(macChromePath) ? macChromePath : undefined);

mkdirSync(workDir, { recursive: true });

export default defineConfig({
  expect: {
    timeout: 10_000,
  },
  forbidOnly: Boolean(process.env.CI),
  fullyParallel: false,
  outputDir: resolve(workDir, 'artifacts'),
  reporter: [
    ['list'],
    ['json', { outputFile: resolve(workDir, 'results.json') }],
  ],
  retries: process.env.CI ? 1 : 0,
  testDir: './e2e',
  timeout: 60_000,
  use: {
    ...devices['Desktop Chrome'],
    baseURL,
    launchOptions: executablePath ? { executablePath } : undefined,
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
    video: 'retain-on-failure',
  },
  webServer: {
    command: `bash -lc 'rm -f "${databasePath}" "${databasePath}-shm" "${databasePath}-wal"; exec cargo run --manifest-path "${resolve(repositoryRoot, 'Cargo.toml')}" -p cloud-api'`,
    env: {
      ...process.env,
      EDGEOPS_API_AUTH_MODE: 'disabled',
      EDGEOPS_BOOTSTRAP_MODE: 'empty',
      EDGEOPS_CLOUD_DB: `sqlite://${databasePath}?mode=rwc`,
      EDGEOPS_CONSOLE_DIST: resolve(repositoryRoot, 'web/console/dist'),
      EDGEOPS_GATEWAY_ADDR: `127.0.0.1:${gatewayPort}`,
      EDGEOPS_HTTP_ADDR: `127.0.0.1:${httpPort}`,
      RUST_LOG: 'cloud_api=info',
    },
    reuseExistingServer: false,
    stderr: 'pipe',
    stdout: 'pipe',
    timeout: 120_000,
    url: `${baseURL}/health/ready`,
  },
  workers: 1,
});
