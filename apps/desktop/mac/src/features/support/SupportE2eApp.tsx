import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { desktop } from '../../ipc/client';
import type { Locale } from '../../i18n';
import { AboutPanel } from './AboutPanel';

function waitFor<T>(read: () => T | null | false, timeoutMs = 15000): Promise<T> {
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const poll = () => {
      try {
        const value = read();
        if (value) return resolve(value);
        if (Date.now() - started > timeoutMs) {
          const view = document.querySelector('.about-panel')?.textContent?.replace(/\s+/g, ' ').slice(0, 1200) ?? '无关于面板';
          return reject(new Error(`等待界面状态超时；当前界面：${view}`));
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

function setInput(input: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const prototype = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
  if (!setter) throw new Error('无法设置验收输入');
  setter.call(input, value);
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

async function runAcceptance(setLocale: (locale: Locale) => void) {
  const originalSubmit = desktop.submitFeedback;
  const originalSelect = desktop.selectLocalFile;
  let submission: { description: string; contact: string; attachment: string | null } | null = null;
  desktop.submitFeedback = async (description, contact, attachment) => {
    submission = { description, contact, attachment };
    return { feedbackId: 'native-ui-feedback-id', status: 'pending' };
  };
  desktop.selectLocalFile = async () => '/tmp/sparrowshell-native-feedback.txt';

  try {
    await waitFor(() => /v\d+\.\d+\.\d+/.test(document.body.textContent ?? '') && document.body.textContent?.includes('macOS · Apple Silicon') ? true : null);
    clickButton('检查更新');
    await waitFor(() => document.querySelector('.about-update-status')?.textContent?.includes('当前已是最新版本') ? true : null, 30000);

    clickButton('服务条款');
    await waitFor(() => document.querySelector('[role="dialog"]')?.textContent?.includes('自动执行所有命令模式不会追加确认') ? true : null);
    clickButton('关闭');
    await waitFor(() => !document.querySelector('[role="dialog"]') ? true : null);
    clickButton('隐私政策');
    await waitFor(() => document.querySelector('[role="dialog"]')?.textContent?.includes('不自动收集终端、服务器配置或 AI 会话') ? true : null);
    clickButton('关闭');

    clickButton('问题反馈');
    const textarea = await waitFor(() => document.querySelector<HTMLTextAreaElement>('.feedback-form textarea'));
    const contact = await waitFor(() => document.querySelector<HTMLInputElement>('.feedback-form input'));
    setInput(textarea, '原生客户端问题反馈界面验收');
    setInput(contact, 'native-ui@example.com');
    clickButton('选择文件');
    await waitFor(() => document.querySelector('.feedback-attachment')?.textContent?.includes('sparrowshell-native-feedback.txt') ? true : null);
    clickButton('提交反馈');
    await waitFor(() => document.querySelector('.feedback-success')?.textContent?.includes('native-ui-feedback-id') ? true : null);
    if (!submission) throw new Error('反馈表单未调用提交能力');
    const captured = submission as { description: string; contact: string; attachment: string | null };
    if (captured.description !== '原生客户端问题反馈界面验收'
        || captured.contact !== 'native-ui@example.com'
        || captured.attachment !== '/tmp/sparrowshell-native-feedback.txt') {
      throw new Error(`反馈字段未正确传递：${JSON.stringify(captured)}`);
    }

    document.documentElement.dataset.theme = 'light';
    await waitFor(() => getComputedStyle(document.documentElement).getPropertyValue('--bg').trim() === '#fff' ? true : null);
    setLocale('en');
    await waitFor(() => document.querySelector('.about-facts')?.textContent?.includes('Version') ? true : null);
    clickButton('Privacy Policy');
    await waitFor(() => document.querySelector('[role="dialog"]')?.textContent?.includes('terminal, server configuration and AI conversations are not collected automatically') ? true : null);
    clickButton('Close');
    document.documentElement.dataset.theme = 'dark';

    const report = {
      status: 'passed',
      checks: [
        'version-and-platform', 'real-native-update-check', 'chinese-terms-and-privacy',
        'feedback-description-contact-attachment', 'feedback-success-receipt',
        'light-theme-token', 'english-about-and-privacy',
      ],
    };
    await invoke('ai_e2e_report', { report: JSON.stringify(report) });
  } finally {
    desktop.submitFeedback = originalSubmit;
    desktop.selectLocalFile = originalSelect;
  }
}

export function SupportE2eApp() {
  const [locale, setLocale] = useState<Locale>('zh');
  const [status, setStatus] = useState('正在运行关于与反馈原生 UI 验收…');
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    document.documentElement.dataset.theme = 'dark';
    void runAcceptance(setLocale)
      .then(() => setStatus('关于与反馈原生 UI 验收通过'))
      .catch(async (reason: unknown) => {
        const message = reason instanceof Error ? reason.message : String(reason);
        setStatus(`关于与反馈原生 UI 验收失败：${message}`);
        await invoke('ai_e2e_report', { report: JSON.stringify({ status: 'failed', message }) }).catch(() => {});
      });
  }, []);

  return <main className="app-shell" style={{ padding: 24 }}>
    <section className="modal" style={{ margin: 'auto', width: 'min(680px, 94vw)' }}>
      <header><h2>{locale === 'zh' ? '关于' : 'About'}</h2><span role="status">{status}</span></header>
      <AboutPanel key={locale} locale={locale} />
    </section>
  </main>;
}
