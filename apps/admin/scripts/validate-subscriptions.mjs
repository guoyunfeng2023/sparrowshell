import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const port = 4187;
const origin = `http://127.0.0.1:${port}`;
const server = spawn(process.execPath, ['node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], {
  cwd: root,
  stdio: ['ignore', 'pipe', 'pipe'],
});
let serverOutput = '';
server.stdout.on('data', chunk => { serverOutput += chunk; });
server.stderr.on('data', chunk => { serverOutput += chunk; });
let browser;
const sleep = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));

function json(route, body) {
  return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
}

function period(index, overrides = {}) {
  return {
    id: `period-${String(index).padStart(3, '0')}`,
    licenseId: `license-${String(index).padStart(3, '0')}`,
    bound: index % 2 === 1,
    boundDeviceId: index % 2 === 1 ? `device-${String(index).padStart(3, '0')}` : null,
    plan: index % 2 === 1 ? 'personal' : 'high',
    quota: index % 2 === 1 ? 200 : 1000,
    used: 36,
    remaining: index % 2 === 1 ? 164 : 964,
    exhausted: false,
    status: index % 5 === 0 ? 'expiring' : index % 3 === 0 ? 'scheduled' : 'active',
    autoRenew: false,
    periodStart: '2026-11-01T00:00:00Z',
    periodEnd: '2026-12-01T00:00:00Z',
    createdAt: '2026-11-01T00:00:00Z',
    ...overrides,
  };
}

try {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    if (server.exitCode != null) throw new Error(`Vite 启动失败：${serverOutput}`);
    try {
      const response = await fetch(origin);
      if (response.ok) break;
    } catch {
      if (attempt === 59) throw new Error(`等待 Vite 超时：${serverOutput}`);
      await sleep(150);
    }
  }

  browser = await chromium.launch({
    ...(process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {}),
    headless: true,
  });
  const page = await browser.newPage({ viewport: { width: 1500, height: 1000 }, deviceScaleFactor: 1 });
  const requests = [];
  const errors = [];
  page.on('console', message => { if (message.type() === 'error') errors.push(`console: ${message.text()}`); });
  page.on('pageerror', error => errors.push(`pageerror: ${error.message}`));
  page.on('response', response => { if (response.status() === 404) errors.push(`404: ${response.url()}`); });

  await page.route('**/v1/admin/**', async route => {
    const request = route.request();
    const url = new URL(request.url());
    requests.push(`${request.method()} ${url.pathname}${url.search}`);
    if (url.pathname === '/v1/admin/auth/login') {
      return json(route, {
        token: 'test-token', sessionId: 'test-session', expiresAt: '2026-11-10T12:00:00Z',
        admin: { email: 'admin@example.com', role: 'SUPER_ADMIN' },
      });
    }
    if (url.pathname === '/v1/admin/dashboard') {
      return json(route, {
        users: { current: 1, cumulative: 1, new: 0, states: [] },
        devices: { validSessions: 0 }, sync: {}, ai: {}, jobs: [], releases: [],
        metricVersion: 'test', generatedAt: '2026-11-10T08:00:00Z',
      });
    }
    if (url.pathname === '/v1/admin/billing/subscriptions/summary') {
      return json(route, { total: 45, active: 8, scheduled: 3, expired: 33, expiring: 2, exhausted: 1, personalActive: 5, highActive: 3 });
    }
    if (url.pathname === '/v1/admin/billing/subscriptions') {
      const offset = Number(url.searchParams.get('offset') || 0);
      const query = url.searchParams.get('query') || '';
      if (query === 'no-results') return json(route, { items: [], total: 0, limit: 20, offset });
      if (query || url.searchParams.get('status') || url.searchParams.get('plan')) {
        return json(route, { items: [period(1, { id: 'filtered-period', status: url.searchParams.get('status') || 'active', plan: url.searchParams.get('plan') || 'personal' })], total: 1, limit: 20, offset: 0 });
      }
      const count = Math.min(20, 45 - offset);
      return json(route, { items: Array.from({ length: count }, (_, index) => period(offset + index + 1)), total: 45, limit: 20, offset });
    }
    if (url.pathname === '/v1/admin/billing/orders') {
      return json(route, { items: [
        { id: 'order-001', alipayTradeNo: 'trade-001', plan: 'personal', amount: 9.9, status: 'paid', licenseId: 'license-001', licenseBound: true, createdAt: '2026-11-01T00:00:00Z', paidAt: '2026-11-01T00:01:00Z', expiresAt: '2026-11-01T00:30:00Z' },
        { id: 'order-002', alipayTradeNo: 'trade-002', plan: 'high', amount: 49, status: 'paid', licenseId: 'license-002', licenseBound: false, createdAt: '2026-11-02T00:00:00Z', paidAt: '2026-11-02T00:01:00Z', expiresAt: '2026-11-02T00:30:00Z' },
        { id: 'order-003', alipayTradeNo: null, plan: 'personal', amount: 9.9, status: 'pending', licenseId: null, licenseBound: null, createdAt: '2026-11-03T00:00:00Z', paidAt: null, expiresAt: '2026-11-03T00:30:00Z' },
      ], total: 3 });
    }
    if (url.pathname === '/v1/admin/billing/licenses') {
      return json(route, { items: [
        { id: 'license-001', activated: true, activatedAt: '2026-11-01T00:02:00Z', bound: true, boundDeviceId: 'device-001', plan: 'personal', quota: 200, used: 36, remaining: 164, periodEnd: '2026-12-01T00:00:00Z', updatedAt: '2026-11-10T00:00:00Z' },
        { id: 'license-002', activated: false, activatedAt: null, bound: false, boundDeviceId: null, latestPlan: 'high', latestPeriodEnd: '2026-12-01T00:00:00Z', updatedAt: '2026-11-10T00:00:00Z' },
      ], total: 2 });
    }
    return json(route, {});
  });

  await page.goto(origin, { waitUntil: 'networkidle' });
  await page.getByLabel('管理员邮箱', { exact: true }).fill('admin@example.com');
  await page.getByLabel('登录密码', { exact: true }).fill('test-password');
  await page.getByRole('button', { name: '进入管理后台' }).click();
  await page.getByRole('button', { name: '充值与订阅', exact: true }).click();
  await page.getByText('order-001', { exact: true }).waitFor();
  await page.locator('tr').filter({ hasText: 'order-001' }).getByText('已绑定', { exact: true }).waitFor();
  await page.locator('tr').filter({ hasText: 'order-002' }).getByText('未绑定', { exact: true }).waitFor();
  const pendingOrderText = await page.locator('tr').filter({ hasText: 'order-003' }).textContent();
  assert.ok(pendingOrderText.includes('支付后生成'), '未支付订单未展示支付后生成');
  assert.ok(!pendingOrderText.includes('未绑定') && !pendingOrderText.includes('已绑定'), '未支付订单不应误标绑定状态');
  await mkdir(path.join(root, 'artifacts'), { recursive: true });
  await sleep(350);
  await page.screenshot({ path: path.join(root, 'artifacts', 'orders-license-binding-desktop.png'), fullPage: true });
  assert.equal(requests.some(value => value.includes('/billing/subscriptions')), false, '订单页不应请求订阅接口');
  console.log('PASS: 订单许可证已绑定、未绑定及未支付状态展示均通过');

  await page.getByRole('button', { name: '订阅周期', exact: true }).click();
  await page.getByText('period-001', { exact: true }).waitFor();
  await page.locator('.metric-card').filter({ hasText: '生效中' }).getByText('8', { exact: true }).waitFor();
  await page.getByText('共 45 个周期 · 已到期 33', { exact: true }).waitFor();
  await page.getByText('手动续费，不自动扣款', { exact: false }).first().waitFor();

  await page.getByLabel('状态筛选').selectOption('scheduled');
  await page.getByText('filtered-period', { exact: true }).waitFor();
  await page.getByLabel('套餐筛选').selectOption('high');
  await page.getByLabel('搜索记录').fill('needle');
  await sleep(150);
  const filtered = requests.filter(value => value.includes('/billing/subscriptions?'));
  assert.ok(filtered.some(value => value.includes('status=scheduled') && value.includes('plan=high') && value.includes('query=needle')), '组合筛选参数不完整');

  await page.getByLabel('搜索记录').fill('');
  await page.getByLabel('状态筛选').selectOption('');
  await page.getByLabel('套餐筛选').selectOption('');
  await page.getByText('period-001', { exact: true }).waitFor();
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await page.getByText('period-021', { exact: true }).waitFor();
  assert.ok(requests.some(value => value.includes('/billing/subscriptions?') && value.includes('limit=20') && value.includes('offset=20')), '第二页未请求 offset=20');
  await page.getByRole('button', { name: '下一页', exact: true }).click();
  await page.getByText('period-041', { exact: true }).waitFor();
  assert.equal(await page.getByRole('button', { name: '下一页', exact: true }).isDisabled(), true, '最后一页应禁用下一页');
  await page.getByRole('button', { name: '上一页', exact: true }).click();
  await page.getByText('period-021', { exact: true }).waitFor();

  await page.getByLabel('搜索记录').fill('no-results');
  await page.getByText('暂无订阅周期', { exact: true }).waitFor();
  assert.equal(await page.getByRole('button', { name: '下一页', exact: true }).isDisabled(), true, '空结果应禁用下一页');
  await page.getByLabel('搜索记录').fill('');
  await page.getByText('period-001', { exact: true }).waitFor();

  await page.getByRole('button', { name: '充值订单', exact: true }).click();
  await page.getByText('order-001', { exact: true }).waitFor();
  await page.getByRole('button', { name: '许可证', exact: true }).click();
  const activatedRow = page.locator('tr').filter({ hasText: 'license-001' });
  await activatedRow.getByText('已激活', { exact: true }).waitFor();
  assert.ok((await activatedRow.textContent()).includes('08:02:00'), '已激活许可证未展示首次激活时间');
  await page.locator('tr').filter({ hasText: 'license-002' }).getByText('未激活', { exact: true }).waitFor();
  await mkdir(path.join(root, 'artifacts'), { recursive: true });
  await page.screenshot({ path: path.join(root, 'artifacts', 'licenses-activation-desktop.png'), fullPage: true });
  await page.getByRole('button', { name: '订阅周期', exact: true }).click();
  await page.getByText('period-001', { exact: true }).waitFor();
  const summaryBeforeRefresh = requests.filter(value => value.includes('/billing/subscriptions/summary')).length;
  await page.getByRole('button', { name: '刷新', exact: true }).click();
  await sleep(200);
  assert.equal(requests.filter(value => value.includes('/billing/subscriptions/summary')).length, summaryBeforeRefresh + 1, '刷新应更新汇总');

  await page.screenshot({ path: path.join(root, 'artifacts', 'subscriptions-desktop.png'), fullPage: true });
  assert.deepEqual(errors, [], '浏览器存在控制台、运行时或 404 错误');
  console.log('PASS: 登录、订阅汇总、列表、组合筛选、20 条分页、空状态、刷新及订单/许可证回归均通过');
  console.log('PASS: 许可证已激活、未激活与首次激活时间展示均通过');
  console.log('PASS: 无控制台错误、页面运行时错误或 404');
  console.log(`SCREENSHOT: ${path.join(root, 'artifacts', 'subscriptions-desktop.png')}`);
  console.log(`SCREENSHOT: ${path.join(root, 'artifacts', 'licenses-activation-desktop.png')}`);
} finally {
  if (browser) await browser.close();
  if (server.exitCode == null) {
    server.kill('SIGTERM');
    await Promise.race([
      new Promise(resolve => server.once('exit', resolve)),
      sleep(3000).then(() => { if (server.exitCode == null) server.kill('SIGKILL'); }),
    ]);
  }
}
