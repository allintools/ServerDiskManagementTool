import { test, expect } from '@playwright/test'

test('fleet renders real-shaped inventory and opens a server detail', async ({ page }) => {
  await page.goto('/#/fleet')
  await expect(page.locator('.fleet-page')).toBeVisible()
  await expect(page.locator('.server-table tbody tr')).toHaveCount(5)
  await expect(page.locator('.kpi-card').first()).toContainText('在线服务器')
  await page.getByText('web-01', { exact: true }).first().click()
  await expect(page.locator('.server-detail-page')).toBeVisible()
  await expect(page.locator('.hero-name h2')).toHaveText('web-01')
  await expect(page.locator('.tab')).toHaveCount(4)
  await page.getByRole('button', { name: '磁盘与卷' }).click()
  await expect(page.locator('.server-detail-page')).toContainText('/data')
  await page.getByRole('button', { name: '空间分析' }).click()
  await expect(page.getByRole('button', { name: '开始分析' })).toBeVisible()
})

test('add server dialog creates a server in demo mode', async ({ page }) => {
  await page.goto('/#/servers')
  await page.getByRole('button', { name: '添加服务器' }).click()
  await expect(page.locator('.add-server-modal')).toBeVisible()
  await page.locator('.add-server-modal input').first().fill('edge-01')
  await page.locator('.add-server-modal input').nth(3).fill('192.0.2.42')
  await page.locator('.add-server-modal input').nth(4).fill('22')
  await page.getByRole('button', { name: '保存并测试连接' }).click()
  await expect(page.locator('.add-server-modal')).toHaveCount(0)
  await expect(page.locator('.server-table tbody')).toContainText('edge-01')
})

test('server list can remove an added server', async ({ page }) => {
  await page.goto('/#/servers')
  await expect(page.locator('.server-table tbody tr')).toHaveCount(5)
  page.once('dialog', dialog => dialog.accept())
  await page.getByRole('button', { name: '删除服务器 web-01' }).click()
  await expect(page.locator('.server-table tbody tr')).toHaveCount(4)
  await expect(page.locator('.server-table tbody')).not.toContainText('web-01')
})
