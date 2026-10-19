import { test, expect } from '@playwright/test'

test('analysis, reclaim candidate and cleanup plan execute flow', async ({ page }) => {
  await page.goto('/#/servers/srv-1/usage')
  await page.getByRole('button', { name: '开始分析' }).click()
  await expect(page.locator('.treemap-cell').first()).toBeVisible()
  await expect(page.locator('.usage-table-wrap tbody tr').first()).toBeVisible()

  await page.getByRole('button', { name: '检测' }).click()
  await expect(page.locator('.reclaim-row')).toHaveCount(4)
  await page.locator('.reclaim-row input').first().check()
  await page.getByRole('button', { name: '生成回收计划' }).click()
  await expect(page.locator('.cleanup-modal')).toBeVisible()
  await expect(page.locator('.plan-item')).toHaveCount(1)
  await page.locator('.cleanup-modal .check-line input').check()
  await page.getByRole('button', { name: '确认执行' }).click()
  await expect(page.locator('.cleanup-modal')).toHaveCount(0)
  await expect(page.locator('.toast')).toContainText('清理任务', { timeout: 15_000 })
})
