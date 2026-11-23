import { Component } from 'react';
import type { ErrorInfo, ReactNode } from 'react';

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

export class AppErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('SparrowShell interface rendering failed', error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    const locale = document.documentElement.lang.startsWith('zh') ? 'zh' : 'en';
    return <main className="fatal-error" role="alert">
      <div>
        <h1>{locale === 'zh' ? '界面出现了异常' : 'The interface encountered an error'}</h1>
        <p>{locale === 'zh' ? '本地服务器配置仍保留。请重新加载界面后继续操作。' : 'Your local server configurations are preserved. Reload the interface to continue.'}</p>
        <details><summary>{locale === 'zh' ? '错误详情' : 'Error details'}</summary><code>{this.state.error.message}</code></details>
        <button className="button primary" onClick={() => window.location.reload()}>{locale === 'zh' ? '重新加载' : 'Reload'}</button>
      </div>
    </main>;
  }
}
