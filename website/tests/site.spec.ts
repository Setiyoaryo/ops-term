import { test, expect } from '@playwright/test';

const errors: string[] = [];
test.beforeEach(async ({ page }) => {
  errors.length = 0;
  page.on('pageerror', error => errors.push(error.message));
  page.on('response', response => { if (response.status() >= 400) errors.push(`${response.status()} ${response.url()}`); });
  await page.goto('./');
  await page.evaluate(() => document.fonts.ready);
});
test.afterEach(() => expect(errors).toEqual([]));

test('real release links, responsive layout and accessible navigation', async ({ page }, info) => {
  await expect(page.getByRole('heading', { level: 1 })).toContainText('Every connection.');
  await expect(page.getByRole('link', { name: 'Download for Linux' })).toHaveAttribute('href', /https:\/\/github.com\/Setiyoaryo\/ops-term\/releases\/download\/v\d+\.\d+\.\d+\/opsterm-egui-/);
  await page.screenshot({ path: `/tmp/opsterm-pages-verification/${info.project.name}-hero.png` });
  const menu = page.getByRole('button', { name: 'Open navigation' });
  if (await menu.isVisible()) {
    await menu.click();
    await expect(page.getByRole('button', { name: 'Close navigation' })).toHaveAttribute('aria-expanded', 'true');
  }
  await page.getByRole('navigation').getByRole('link', { name: 'Features' }).click();
  await expect(page).toHaveURL(/#features$/);
  await expect(page.getByRole('heading', { name: 'For real operations. For everyday work.' })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await page.screenshot({ path: `/tmp/opsterm-pages-verification/${info.project.name}-full.png`, fullPage: true });
});

test('palette and session previews work with keyboard tabs', async ({ page }) => {
  await page.getByRole('button', { name: 'Warm', exact: true }).click();
  await expect(page.locator('.workspace')).toHaveAttribute('data-palette', 'warm');
  await expect(page.getByRole('button', { name: 'Warm', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('tab', { name: 'Local shell' }).click();
  await expect(page.getByRole('tabpanel', { name: 'Local shell' })).toContainText('workstation');
  await page.getByRole('tab', { name: 'Local shell' }).press('ArrowLeft');
  await expect(page.getByRole('tab', { name: 'edge-01' })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('tabpanel', { name: 'edge-01' })).toContainText('show interfaces status');
});

test('installation copy matches selected tab and status resets', async ({ page }) => {
  await page.evaluate(() => { Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async (text: string) => { (window as any).copiedInstall = text; } } }); });
  const copy = page.getByRole('button', { name: 'Copy install commands' });
  await copy.click();
  expect(await page.evaluate(() => (window as any).copiedInstall)).toContain('sha256sum -c');
  await expect(copy).toContainText('Copied');
  await page.getByRole('tab', { name: 'Build from source' }).click();
  await expect(copy).toHaveText(/Copy\s*⧉/, { timeout: 500 });
  await copy.click();
  expect(await page.evaluate(() => (window as any).copiedInstall)).toContain('git clone https://github.com/Setiyoaryo/ops-term.git');
});

test('theme persists, FAQ opens, and clipboard failures are honest', async ({ page }) => {
  await page.getByRole('button', { name: 'Switch to light theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByText('How do automatic updates work?', { exact: false }).click();
  await expect(page.getByText('The app checks GitHub Releases for new signed versions.', { exact: false })).toBeVisible();
  await page.evaluate(() => { Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async () => { throw new Error('blocked'); } } }); });
  await page.getByRole('button', { name: 'Copy install commands' }).click();
  await expect(page.getByRole('status')).toContainText('copy them manually');
});
