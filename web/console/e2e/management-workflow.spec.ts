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
  const disposableProjectDialog = page.getByRole('dialog', { name: '项目详情' });
  await expect(disposableProjectDialog).toBeVisible();
  await disposableProjectDialog.getByLabel('项目名称').fill('浏览器删除验收项目');
  await disposableProjectDialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(disposableProjectDialog.getByRole('status')).toHaveText('已保存');
  await page.keyboard.press('Escape');
  await expect(disposableProjectDialog).toBeHidden();

  const disposableProjectRow = page
    .getByRole('row')
    .filter({ hasText: '浏览器删除验收项目' });
  await expect(disposableProjectRow).toBeVisible();
  await disposableProjectRow.getByRole('button', { name: '详情', exact: true }).click();
  await expect(disposableProjectDialog).toBeVisible();
  await disposableProjectDialog.getByRole('button', { name: '删除', exact: true }).click();
  const projectDeleteConfirmation = disposableProjectDialog.getByRole('alertdialog', {
    name: '确认删除项目',
  });
  await expect(projectDeleteConfirmation).toBeVisible();
  await projectDeleteConfirmation
    .getByRole('button', { name: '确认删除', exact: true })
    .click();
  await expect(disposableProjectDialog).toBeHidden();
  await expect(disposableProjectRow).toHaveCount(0);

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
  await productDialog.getByRole('tab', { name: '协议连接' }).click();
  await productDialog.getByRole('button', { name: '管理', exact: true }).first().click();

  const connectionWorkspace = page.getByRole('dialog', { name: /协议连接工作区/ });
  await expect(connectionWorkspace).toBeVisible();
  await connectionWorkspace.getByRole('tab', { name: /绑定点位/ }).click();
  const pointSetRow = connectionWorkspace
    .getByRole('row')
    .filter({ hasText: '浏览器验收点位集' });
  await pointSetRow.getByRole('button', { name: '绑定', exact: true }).click();
  await expect(pointSetRow.getByText('已绑定', { exact: true })).toBeVisible();
  await connectionWorkspace.getByRole('tab', { name: /采集编排/ }).click();
  await expect(connectionWorkspace.getByLabel('采集编排画布')).toBeVisible();
  await connectionWorkspace
    .getByRole('button', { name: '从 e2e_pressure 连线' })
    .click();
  await connectionWorkspace
    .getByRole('button', { name: '连接到 多点合并' })
    .click();
  await connectionWorkspace
    .getByRole('button', { name: '从 多点合并 连线' })
    .click();
  await connectionWorkspace
    .getByRole('button', { name: '连接到 MQTT 输出 1' })
    .click();
  await connectionWorkspace.screenshot({
    path: test.info().outputPath('protocol-connection-workspace.png'),
  });
  await page.keyboard.press('Escape');
  await expect(connectionWorkspace).toBeHidden();

  await productDialog
    .getByRole('button', { name: '保存并同步', exact: true })
    .click();
  await expect(productDialog.getByRole('status')).toHaveText(
    /已保存并触发自动同步|已保存，配置待完善/,
  );
  await expect(
    productDialog.getByRole('tab', { name: '发布策略' }),
  ).toHaveCount(0);

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

  await edgeRow.getByRole('button', { name: /MQTT 配置/ }).click();
  const mqttDialog = page.getByRole('dialog', { name: '边端 MQTT 配置' });
  await expect(mqttDialog).toBeVisible();
  await expect(mqttDialog.getByText('上报 Topic 与触发策略由采集编排定义')).toBeVisible();
  await expect(mqttDialog.getByRole('button', { name: 'MQTT 3.1.1' })).toBeVisible();
  await mqttDialog.getByRole('button', { name: 'MQTT 5.0' }).click();
  await mqttDialog.getByRole('tab', { name: 'MQTT 5' }).click();
  await expect(mqttDialog.getByLabel('Clean Start')).toBeVisible();
  await expect(mqttDialog.getByLabel('Session Expiry（秒）')).toBeVisible();
  await expect(mqttDialog.getByLabel('Receive Maximum')).toBeVisible();
  await expect(mqttDialog.getByLabel('Maximum Packet Size（字节）')).toBeVisible();
  await expect(mqttDialog.getByLabel('Topic Alias Maximum')).toBeVisible();
  await expect(mqttDialog.getByText('默认 Topic 模板')).toHaveCount(0);
  await expect(mqttDialog.getByText('QoS', { exact: true })).toHaveCount(0);
  await expect(mqttDialog.getByText('批量条数')).toHaveCount(0);
  await expect(mqttDialog.getByText('刷新间隔(ms)')).toHaveCount(0);
  await expect
    .poll(async () => (await mqttDialog.boundingBox())?.width ?? 0)
    .toBeGreaterThan(800);
  await expect
    .poll(() =>
      page.locator('.modal-backdrop').evaluate((element) =>
        getComputedStyle(element).backdropFilter,
      ),
    )
    .toBe('none');
  await mqttDialog.screenshot({
    path: test.info().outputPath('mqtt5-connection-dialog.png'),
  });
  await mqttDialog.getByRole('tab', { name: '遗嘱消息' }).click();
  await mqttDialog.getByLabel('启用遗嘱消息').check();
  await expect(mqttDialog.getByLabel('Will Topic')).toBeVisible();
  await expect(mqttDialog.getByLabel('Will Payload')).toBeVisible();
  await expect(mqttDialog.getByLabel('Will Delay（秒）')).toBeVisible();
  await expect(mqttDialog.getByLabel('Message Expiry（秒）')).toBeVisible();
  await mqttDialog.screenshot({
    path: test.info().outputPath('mqtt5-last-will-dialog.png'),
  });
  await page.keyboard.press('Escape');
  await expect(mqttDialog).toBeHidden();

  await page.getByRole('button', { name: '审计日志', exact: true }).click();
  await expect(
    page.getByRole('heading', { level: 2, name: '审计日志', exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('table')).toBeVisible();
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('已同步');
  const auditDetailButton = page.getByTitle('查看详情').first();
  await expect(auditDetailButton).toBeVisible();
  await auditDetailButton.click();
  const auditDialog = page.getByRole('dialog', { name: '审计事件详情' });
  await expect(auditDialog).toBeVisible();
  await expect(auditDialog.getByText('只读审计记录')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(auditDialog).toBeHidden();

  await page.screenshot({
    fullPage: true,
    path: test.info().outputPath('management-workflow.png'),
  });
  expect(browserErrors).toEqual([]);
});
