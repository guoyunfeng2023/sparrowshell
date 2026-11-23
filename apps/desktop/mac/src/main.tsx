import React from 'react';
import ReactDOM from 'react-dom/client';
import '@xterm/xterm/css/xterm.css';
import './styles.css';
import App from './App';
import { AppErrorBoundary } from './features/support/AppErrorBoundary';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isDesktop } from './ipc/client';

const root = ReactDOM.createRoot(document.getElementById('root')!);
const revealWindow = () => {
  if (!isDesktop) return;
  window.setTimeout(() => {
    void getCurrentWindow().show().catch(() => {});
  }, 50);
};
const supportE2e = import.meta.env.VITE_SPARROWSHELL_SUPPORT_E2E === '1';
const accountE2e = import.meta.env.VITE_SPARROWSHELL_ACCOUNT_E2E === '1';
const nativeE2e = import.meta.env.VITE_SPARROWSHELL_E2E === '1'
  || (import.meta.env.DEV && new URLSearchParams(window.location.search).has('sparrowshell-e2e'));

if (accountE2e) {
  void import('./features/account/AccountE2eApp').then(({ AccountE2eApp }) => root.render(<AccountE2eApp />));
} else if (supportE2e) {
  void import('./features/support/SupportE2eApp').then(({ SupportE2eApp }) => root.render(<SupportE2eApp />));
} else if (nativeE2e) {
  void import('./features/ai/AiE2eApp').then(({ AiE2eApp }) => root.render(<AiE2eApp />));
} else {
  root.render(<React.StrictMode><AppErrorBoundary><App /></AppErrorBoundary></React.StrictMode>);
}
revealWindow();
