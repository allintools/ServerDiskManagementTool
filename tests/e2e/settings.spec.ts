import { test, expect } from '@playwright/test'

test('settings loads SSH diagnostics and saves thresholds', async ({ page }) => {
  await page.goto('/#/settings')
  await expect(page.locator('.settings-page')).toBeVisible()
  await expect(page.locator('.diagnostics-grid')).toContainText('OpenSSH')
  await expect(page.locator('.known-host-row')).toHaveCount(1)
  const warn = page.locator('.settings-section input[type=number]').first()
  await warn.fill('81')
  await page.getByRole('button', { name: '保存设置' }).click()
  await expect(page.locator('.toast')).toContainText('设置已保存')
})
