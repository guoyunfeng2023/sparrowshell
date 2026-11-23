import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));

describe('account IPC locale', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
    vi.mocked(invoke).mockResolvedValue(undefined);
  });

  afterEach(() => vi.unstubAllGlobals());

  it('sends English locale with the verification code request', async () => {
    const { desktop } = await import('./client');
    await desktop.accountSendVerificationCode('person@example.test', 'register', 'en');

    expect(invoke).toHaveBeenCalledWith('account_send_verification_code', {
      email: 'person@example.test',
      purpose: 'register',
      locale: 'en-US',
    });
  });

  it('sends Chinese locale with the verification code request', async () => {
    const { desktop } = await import('./client');
    await desktop.accountSendVerificationCode('person@example.test', 'password_reset', 'zh');

    expect(invoke).toHaveBeenCalledWith('account_send_verification_code', expect.objectContaining({ locale: 'zh-CN' }));
  });

  it('synchronizes the web theme to the native window background', async () => {
    const { desktop } = await import('./client');
    await desktop.setAppTheme('light');

    expect(invoke).toHaveBeenCalledWith('set_app_theme', { theme: 'light' });
  });
});
