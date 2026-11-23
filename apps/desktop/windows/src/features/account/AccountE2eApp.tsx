import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { desktop } from '../../ipc/client';
import type { AccountAuthResult, AccountSession, AccountStatus, CatalogSnapshot } from '../../types';
import { AccountPanel } from './AccountPanel';

function waitFor<T>(read: () => T | null | false, timeoutMs = 10000): Promise<T> {
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const poll = () => {
      try {
        const value = read();
        if (value) return resolve(value);
        if (Date.now() - started > timeoutMs) {
          const view = document.querySelector('.account-modal')?.textContent?.replace(/\s+/g, ' ').slice(0, 1600) ?? '无账号面板';
          return reject(new Error(`等待账号界面状态超时；当前界面：${view}`));
        }
        window.setTimeout(poll, 50);
      } catch (reason) { reject(reason); }
    };
    poll();
  });
}

function clickButton(text: string) {
  const button = Array.from(document.querySelectorAll<HTMLButtonElement>('button'))
    .find((item) => (item.textContent?.includes(text) || item.getAttribute('aria-label') === text) && !item.disabled);
  if (!button) throw new Error(`找不到按钮：${text}`);
  button.click();
}

function setInput(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
  if (!setter) throw new Error('无法设置账号验收输入');
  setter.call(input, value);
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

function session(userId: 'account-a' | 'account-b'): AccountSession {
  return {
    userId,
    deviceId: 'native-account-e2e-device',
    expiresAt: '2099-01-01T00:00:00Z',
    email: `${userId}@example.com`,
    nickname: userId === 'account-a' ? '账号 A' : '账号 B',
  };
}

function authResult(userId: 'account-a' | 'account-b', requiresChoice: boolean): AccountAuthResult {
  return { session: session(userId), requiresChoice, previousUserId: 'account-a', localFolders: 2, localServers: 5 };
}

async function runAcceptance(showPanel: () => void, remount: () => void, registerCatalogChanged: (callback: () => Promise<void>) => void, setInitialStatus: (status: AccountStatus) => void) {
  const catalogFixture: CatalogSnapshot = {
    folders: Array.from({ length: 2 }, (_, index) => ({ id: `folder-${index}`, parentId: null, name: `文件夹 ${index}`, sortOrder: index, revision: 1, updatedAt: '2026-01-01T00:00:00Z' })),
    servers: Array.from({ length: 5 }, (_, index) => ({ id: `server-${index}`, folderId: null, name: `服务器 ${index}`, host: '127.0.0.1', port: 22, username: 'root', authType: 'password', privateKeyPathHint: null, jumpServerId: null, sortOrder: index, revision: 1, updatedAt: '2026-01-01T00:00:00Z', deletedAt: null })),
  };
  const originals = {
    catalog: desktop.catalog,
    accountStatus: desktop.accountStatus,
    accountProfile: desktop.accountProfile,
    accountLogin: desktop.accountLogin,
    accountLogout: desktop.accountLogout,
    accountSwitch: desktop.accountSwitch,
    accountCancelSwitch: desktop.accountCancelSwitch,
    accountPendingSwitch: desktop.accountPendingSwitch,
    syncStatus: desktop.syncStatus,
    syncConflicts: desktop.syncConflicts,
    onSync: desktop.onSync,
    onSyncError: desktop.onSyncError,
  };
  const originalConfirm = window.confirm;
  let status: AccountStatus = { state: 'signed_in', session: session('account-a') };
  let pending: AccountAuthResult | null = null;
  let loginTarget: 'account-a' | 'account-b' = 'account-a';
  let loginCount = 0;
  let logoutCount = 0;
  let cancelCount = 0;
  let catalogRefreshCount = 0;
  const switchChoices: string[] = [];

  desktop.accountStatus = async () => status;
  desktop.accountProfile = async () => {
    if (!('session' in status)) throw new Error('当前无登录会话');
    return status.session;
  };
  desktop.accountLogin = async () => {
    loginCount += 1;
    const result = authResult(loginTarget, loginTarget === 'account-b');
    status = { state: 'signed_in', session: result.session };
    pending = result.requiresChoice ? result : null;
    return result;
  };
  desktop.accountLogout = async () => {
    logoutCount += 1;
    status = { state: 'signed_out' };
    pending = null;
    return { remoteRevoked: true };
  };
  desktop.accountPendingSwitch = async () => pending;
  desktop.accountCancelSwitch = async () => {
    cancelCount += 1;
    status = { state: 'signed_out' };
    pending = null;
  };
  desktop.accountSwitch = async (choice) => {
    if (!pending) throw new Error('没有待处理的账号切换');
    switchChoices.push(choice);
    pending = null;
    status = { state: 'signed_in', session: session('account-b') };
    return {
      applied: true,
      syncReport: { pushed: 0, conflicts: 0, pulled: 7, snapshotRestored: false },
      syncError: null,
      outbox: 0,
      conflicts: 0,
    };
  };
  desktop.catalog = async () => catalogFixture;
  desktop.syncStatus = async () => ({ cursor: 0, outbox: 0, conflicts: 0 });
  desktop.syncConflicts = async () => [];
  desktop.onSync = async () => () => {};
  desktop.onSyncError = async () => () => {};
  window.confirm = () => { throw new Error('退出登录不应弹出清空或确认提示'); };
  registerCatalogChanged(onCatalogChanged);
  showPanel();

  const login = async (target: 'account-a' | 'account-b') => {
    loginTarget = target;
    const email = await waitFor(() => document.querySelector<HTMLInputElement>('.account-login input[type="email"]'));
    const password = await waitFor(() => document.querySelector<HTMLInputElement>('.account-login input[type="password"]'));
    setInput(email, `${target}@example.com`);
    setInput(password, 'native-e2e-password');
    clickButton('登录并启用同步');
  };

  try {
    await waitFor(() => document.querySelector('.account-summary')?.textContent?.includes('账号 A') ? true : null);
    const syncStats = document.querySelector('.sync-stats')?.textContent?.replace(/\s+/g, '') ?? '';
    if (!syncStats.includes('同步文件夹2') || !syncStats.includes('同步服务器5') || syncStats.includes('同步游标')) {
      throw new Error(`同步统计未显示实际文件夹和服务器数量：${syncStats}`);
    }
    desktop.accountProfile = async () => {
      await new Promise((resolve) => window.setTimeout(resolve, 200));
      return session('account-a');
    };
    setInitialStatus({ state: 'expired', session: session('account-a') });
    remount();
    await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    if (document.querySelector('.account-summary')?.textContent?.includes('登录已过期')) {
      throw new Error('恢复账号状态时不应闪现登录已过期');
    }
    if (!document.querySelector('.account-initial-loading')) throw new Error('恢复过期账号时应显示加载边界');
    await waitFor(() => document.querySelector('.account-summary')?.textContent?.includes('账号 A') ? true : null);
    desktop.accountProfile = async () => {
      if (!('session' in status)) throw new Error('当前无登录会话');
      return status.session;
    };

    clickButton('退出登录');
    await waitFor(() => document.querySelector('.account-login') ? true : null);
    if (logoutCount !== 1) throw new Error('退出登录未正确调用');

    const emptyPasswordEmail = await waitFor(() => document.querySelector<HTMLInputElement>('.account-login input[type="email"]'));
    setInput(emptyPasswordEmail, 'account-a@example.com');
    if (!document.querySelector('.account-login')?.hasAttribute('novalidate')) throw new Error('登录表单不应使用系统原生英文校验');
    clickButton('登录并启用同步');
    await waitFor(() => document.querySelector('.account-modal .error')?.textContent === '请输入账号密码。' ? true : null);
    if (loginCount !== 0) throw new Error('密码为空时不应调用登录接口');

    await login('account-a');
    await waitFor(() => document.querySelector('.account-summary')?.textContent?.includes('账号 A') ? true : null);
    if (document.querySelector('.account-switch-choice')) throw new Error('重新登录同一账号不应显示三选一');

    clickButton('退出登录');
    await waitFor(() => document.querySelector('.account-login') ? true : null);
    await login('account-b');
    await waitFor(() => document.querySelector('.account-switch-choice') ? true : null);
    const choiceText = document.querySelector('.account-switch-choice')?.textContent ?? '';
    for (const label of ['从云端覆盖本地', '本地覆盖云端', '本地云端合并', '2 个文件夹', '5 台服务器']) {
      if (!choiceText.includes(label)) throw new Error(`换号选择缺少内容：${label}`);
    }

    clickButton('取消并退出该账号');
    await waitFor(() => document.querySelector('.account-login') ? true : null);
    await waitFor(() => document.querySelector('.main-shell-probe')?.textContent?.includes('signed_out') ? true : null);
    if (!document.querySelector('.app-shell')) throw new Error('取消切换后主界面不应消失');
    if (cancelCount !== 1 || switchChoices.length !== 0) throw new Error('取消切换不应修改工作区');

    await login('account-b');
    await waitFor(() => document.querySelector('.account-switch-choice') ? true : null);
    desktop.accountPendingSwitch = async () => {
      await new Promise((resolve) => window.setTimeout(resolve, 200));
      return pending;
    };
    setInitialStatus({ state: 'signed_out' });
    remount();
    await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    if (document.querySelector('.account-login')) throw new Error('恢复账号状态时不应闪现未登录表单');
    if (!document.querySelector('.account-initial-loading')) throw new Error('恢复账号状态时应显示加载边界');
    await waitFor(() => document.querySelector('.account-switch-choice')?.textContent?.includes('账号 B') ? true : null);
    desktop.accountPendingSwitch = async () => pending;

    for (const [label, warning] of [
      ['从云端覆盖本地', '当前本机展示的配置将被该账号的云端配置替换'],
      ['本地覆盖云端', '云端独有的文件夹和服务器配置将被删除'],
      ['本地云端合并', '相同实体的内容不一致时会生成待处理冲突'],
    ]) {
      clickButton(label);
      const dialog = await waitFor(() => {
        const candidate = document.querySelector<HTMLElement>('.account-switch-confirm-modal');
        return candidate?.textContent?.includes(label) && candidate.textContent.includes(warning) ? candidate : null;
      });
      if (switchChoices.length !== 0) throw new Error(`${label} 未经确认就执行了同步`);
      const cancel = Array.from(dialog.querySelectorAll<HTMLButtonElement>('button')).find((button) => button.textContent === '取消');
      if (!cancel) throw new Error(`${label} 确认框缺少取消按钮`);
      cancel.click();
      await waitFor(() => !document.querySelector('.account-switch-confirm-modal') ? true : null);
      if (switchChoices.length !== 0 || !document.querySelector('.account-switch-choice')) throw new Error(`${label} 取消确认后不应同步或退出选择`);
    }

    clickButton('从云端覆盖本地');
    await waitFor(() => document.querySelector('.account-switch-confirm-modal') ? true : null);
    clickButton('确认并同步');
    await waitFor(() => document.querySelector('.account-summary')?.textContent?.includes('账号 B') ? true : null);
    if (switchChoices.join(',') !== 'cloud_replace_local') throw new Error('账号切换选项未正确传递');
    if (catalogRefreshCount === 0) throw new Error('切换后未刷新本机目录');

    const report = {
      status: 'passed',
      checks: [
        'login-empty-password-uses-localized-error', 'sync-stats-show-folder-and-server-counts', 'logout-without-confirmation', 'same-account-without-choice', 'different-account-three-choices',
        'full-local-workspace-counts', 'all-switch-choices-require-confirmation', 'cancel-switch-confirmation-does-not-sync', 'cancel-switch-preserves-workspace', 'cancel-switch-keeps-main-shell-mounted',
        'pending-switch-restored-after-remount',
        'expired-initial-status-never-flashes', 'stale-signed-out-status-never-flashes-login',
        'switch-choice-and-catalog-refresh', 'mocked-no-real-cloud-mutation',
      ],
    };
    await invoke('ai_e2e_report', { report: JSON.stringify(report) });
  } finally {
    Object.assign(desktop, originals);
    window.confirm = originalConfirm;
  }

  function onCatalogChanged() {
    catalogRefreshCount += 1;
    return Promise.resolve();
  }
}

export function AccountE2eApp() {
  const [revision, setRevision] = useState(0);
  const [ready, setReady] = useState(false);
  const [status, setStatus] = useState('正在运行账号切换原生 UI 验收…');
  const [liveAccountStatus, setLiveAccountStatus] = useState<AccountStatus>({ state: 'signed_in', session: session('account-a') });
  const started = useRef(false);
  const catalogChanged = useRef<() => Promise<void>>(async () => {});
  const initialStatus = useRef<AccountStatus>({ state: 'signed_in', session: session('account-a') });

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    document.documentElement.dataset.theme = 'dark';
    void runAcceptance(
      () => setReady(true),
      () => setRevision((value) => value + 1),
      (callback) => { catalogChanged.current = callback; },
      (nextStatus) => { initialStatus.current = nextStatus; },
    )
      .then(() => setStatus('账号切换原生 UI 验收通过'))
      .catch(async (reason: unknown) => {
        const message = reason instanceof Error ? reason.message : String(reason);
        setStatus(`账号切换原生 UI 验收失败：${message}`);
        await invoke('ai_e2e_report', { report: JSON.stringify({ status: 'failed', message }) }).catch(() => {});
      });
  }, []);

  return <main className="app-shell" style={{ padding: 24 }}>
    <div className="main-shell-probe">主界面账号状态：{liveAccountStatus.state}</div>
    <span role="status" style={{ position: 'fixed', left: 16, bottom: 12, zIndex: 10000 }}>{status}</span>
    {ready && <AccountPanel
      key={revision}
      locale="zh"
      initialStatus={initialStatus.current}
      onClose={() => setRevision((value) => value + 1)}
      onCatalogChanged={() => catalogChanged.current()}
      onAccountChanged={async () => setLiveAccountStatus(await desktop.accountStatus())}
    />}
  </main>;
}
