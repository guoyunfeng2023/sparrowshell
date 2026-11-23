import { useEffect } from 'react';
import { Download, ShieldAlert, X } from 'lucide-react';
import type { Locale } from '../../i18n';
import type { UpdateCheck } from '../../types';
import { canDismissUpdatePrompt } from './updateFlow';
import './UpdateDialog.css';

const copy = {
  zh: {
    title: '发现新版本', required: '此版本必须更新后才能继续使用 SparrowShell。', version: '新版本', policy: '更新策略', published: '发布时间',
    force: '强制更新', optional: '可选择更新', silent: '不提示更新', notes: '更新内容', emptyNotes: '暂无更新内容。',
    downloadHint: '点击“确定”将通过浏览器打开下载地址。下载后请安装新版本并重新启动客户端。',
    downloaded: '下载地址已打开。安装并重启新版本后，才能继续使用当前客户端。', confirm: '确定', cancel: '取消', opening: '正在打开…',
    missing: '更新下载地址无效，请联系管理员。', close: '关闭',
  },
  en: {
    title: 'New version available', required: 'You must install this update before continuing to use SparrowShell.', version: 'New version', policy: 'Update policy', published: 'Published',
    force: 'Required update', optional: 'Optional update', silent: 'Do not notify', notes: 'What’s new', emptyNotes: 'No release notes provided.',
    downloadHint: 'Select OK to open the download in your browser. Install the new version and restart the app after downloading.',
    downloaded: 'The download is open. Install and restart the new version before continuing to use this app.', confirm: 'OK', cancel: 'Cancel', opening: 'Opening…',
    missing: 'The update download URL is invalid. Contact your administrator.', close: 'Close',
  },
} as const;

interface UpdateDialogProps {
  update: UpdateCheck;
  locale: Locale;
  manual: boolean;
  opening: boolean;
  downloadOpened: boolean;
  error: string;
  onCancel: () => void;
  onConfirm: () => void;
}

export function UpdateDialog({ update, locale, manual, opening, downloadOpened, error, onCancel, onConfirm }: UpdateDialogProps) {
  const t = copy[locale];
  const dismissible = canDismissUpdatePrompt(update, manual);
  const forced = !dismissible;
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      if (dismissible && !opening) onCancel();
    };
    document.addEventListener('keydown', onKeyDown, true);
    return () => document.removeEventListener('keydown', onKeyDown, true);
  }, [dismissible, opening, onCancel]);
  const policy = update.updatePolicy ? t[update.updatePolicy] : null;
  return <div className="modal-backdrop update-backdrop" onMouseDown={(event) => {
    if (dismissible && !opening && event.target === event.currentTarget) onCancel();
  }}>
    <section className="modal update-modal" role={forced ? 'alertdialog' : 'dialog'} aria-modal="true" aria-labelledby="update-dialog-title" aria-describedby="update-dialog-description">
      <header><h2 id="update-dialog-title">{t.title}</h2>{dismissible && <button className="icon" disabled={opening} onClick={onCancel} aria-label={t.close}><X size={17} /></button>}</header>
      <div className="update-content">
        {forced && <p className="update-required" id="update-dialog-description"><ShieldAlert size={17} />{t.required}</p>}
        <dl className="update-facts">
          <div><dt>{t.version}</dt><dd>v{update.version ?? '—'}</dd></div>
          {policy && <div><dt>{t.policy}</dt><dd>{policy}</dd></div>}
          {update.publishedAt && <div><dt>{t.published}</dt><dd>{new Date(update.publishedAt).toLocaleString(locale === 'zh' ? 'zh-CN' : 'en-US')}</dd></div>}
        </dl>
        <section className="update-notes"><h3>{t.notes}</h3><p>{update.releaseNotes || t.emptyNotes}</p></section>
        <p className="update-download-hint">{forced && downloadOpened ? t.downloaded : t.downloadHint}</p>
        {error && <p className="error" role="alert">{error}</p>}
        <footer><>{dismissible && <button className="button secondary" disabled={opening} onClick={onCancel}>{t.cancel}</button>}</><button autoFocus className="button primary" disabled={opening} onClick={onConfirm}><Download size={14} />{opening ? t.opening : t.confirm}</button></footer>
      </div>
    </section>
  </div>;
}
