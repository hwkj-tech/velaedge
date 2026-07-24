import { expect, test } from '@playwright/test';

test('operates the real project, product, and edge enrollment workflow', async ({
  page,
}) => {
  const browserErrors: string[] = [];
  page.on('pageerror', (error) => browserErrors.push(error.message));

  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Dashboard', exact: true })).toBeVisible();
  await expect(page.getByText('local-development')).toBeVisible();

  await page.getByRole('button', { name: '项目管理', exact: true }).click();
  await expect(
    page.getByRole('heading', { level: 2, name: '项目管理', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: '新建项目', exact: true }).click();

  const projectDialog = page.getByRole('dialog', { name: '项目详情' });
  await expect(projectDialog).toBeVisible();
  await projectDialog.getByLabel('项目名称').fill('浏览器验收项目');
  await projectDialog.getByLabel('负责人').fill('e2e-operator');
  await projectDialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(projectDialog.getByRole('status')).toHaveText('已保存');

  await page.keyboard.press('Escape');
  await expect(projectDialog).toBeHidden();
  await expect(
    page.getByRole('cell', { name: '浏览器验收项目', exact: true }),
  ).toBeVisible();

  await page.getByRole('button', { name: '点位管理', exact: true }).click();
  await expect(
    page.getByRole('heading', { level: 2, name: '点位集管理', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: '新建点位集', exact: true }).click();

  const pointSetDialog = page.getByRole('dialog', { name: '新建点位集' });
  await pointSetDialog.getByLabel('点位集 ID').fill('e2e-pump-points');
  await pointSetDialog.getByLabel('点位集名称').fill('浏览器验收点位集');
  await pointSetDialog.getByLabel('点位 1 Point ID').fill('e2e_pressure');
  await pointSetDialog.getByLabel('点位 1 语义 ID').fill('pump.pressure');
  await pointSetDialog.getByLabel('点位 1 地址值').fill('40001');
  await pointSetDialog.getByLabel('点位 1 单位').fill('MPa');
  await pointSetDialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(pointSetDialog).toBeHidden();
  await expect(
    page.getByRole('button', { name: '查看点位集 浏览器验收点位集' }),
  ).toBeVisible();

  await page.getByRole('button', { name: '产品管理', exact: true }).click();
  await page.getByRole('button', { name: '新建产品', exact: true }).click();

  const productDialog = page.getByRole('dialog', { name: '产品配置' });
  await expect(productDialog).toBeVisible();
  await productDialog.getByLabel('产品名称').fill('浏览器验收产品');
  await productDialog.getByRole('tab', { name: '绑定点位' }).click();
  const pointSetRow = productDialog
    .getByRole('row')
    .filter({ hasText: '浏览器验收点位集' });
  await pointSetRow.getByRole('button', { name: '绑定', exact: true }).click();
  await expect(pointSetRow.getByText('已绑定', { exact: true })).toBeVisible();
  await productDialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(productDialog.getByRole('status')).toHaveText('已保存');
  await productDialog.getByRole('tab', { name: '采集编排' }).click();
  await expect(productDialog.getByLabel('采集编排画布')).toBeVisible();
  await productDialog.getByRole('tab', { name: '发布策略' }).click();
  await productDialog.getByRole('button', { name: '发布此版本' }).click();
  await expect(productDialog.getByText('当前版本')).toBeVisible();

  await page.keyboard.press('Escape');
  await expect(productDialog).toBeHidden();
  await expect(
    page.getByRole('cell', { name: '浏览器验收产品', exact: true }),
  ).toBeVisible();

  await page.getByRole('button', { name: '边端管理', exact: true }).click();
  await page.getByRole('button', { name: '新增边端', exact: true }).click();

  const edgeDialog = page.getByRole('dialog', { name: '新增边端' });
  await edgeDialog.getByLabel('边端名称').fill('浏览器验收边端');
  await edgeDialog.getByLabel('站点/分组').fill('e2e/lab');
  await expect(edgeDialog.getByLabel('关联产品')).not.toHaveValue('');
  await edgeDialog.getByRole('button', { name: '生成接入 token' }).click();

  const accessDialog = page.getByRole('dialog', { name: '边端接入信息' });
  await expect(accessDialog).toBeVisible();
  await expect(accessDialog.getByText(/edge-runtime --cloud-gateway-addr/)).toBeVisible();
  await expect(accessDialog.getByText(/edge_[A-Za-z0-9_-]+/).first()).toBeVisible();

  await page.keyboard.press('Escape');
  await expect(accessDialog).toBeHidden();
  await expect(
    page.getByRole('cell', { name: '浏览器验收边端', exact: true }),
  ).toBeVisible();

  const edgeRow = page.getByRole('row').filter({ hasText: '浏览器验收边端' });
  await edgeRow.getByRole('button', { name: /运行监控/ }).click();
  const monitorDialog = page.getByRole('dialog', { name: '边端运行监控' });
  await expect(monitorDialog).toBeVisible();
  await expect(monitorDialog.getByText('运行状态')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(monitorDialog).toBeHidden();

  await page.screenshot({
    fullPage: true,
    path: test.info().outputPath('management-workflow.png'),
  });
  expect(browserErrors).toEqual([]);
});
