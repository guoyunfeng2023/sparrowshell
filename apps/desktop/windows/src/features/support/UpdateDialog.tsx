import { useEffect } from 'react';
import { Download, ShieldAlert, X } from 'lucide-react';
import type { Locale } from '../../i18n';
import type { UpdateCheck, UpdateDownloadProgress } from '../../types';
import { canDismissUpdatePrompt, updateProgressPercent } from './updateFlow';
import './UpdateDialog.css';

const copy = {
  zh: {
    title: '发现新版本', required: '此版本必须更新后才能继续使用 SparrowShell。', version: '新版本', policy: '更新策略', published: '发布时间',
    force: '强制更新', optional: '可选择更新', silent: '不提示更新', notes: '更新内容', emptyNotes: '暂无更新内容。',
    downloadHint: '点击“立即更新”将在应用内下载并自动安装，完成后 SparrowShell 会自动重启。',
    downloading: '正在下载更新…', installing: '下载完成，正在安装，应用将自动重启…',
    confirm: '立即更新', cancel: '取消', working: '正在更新…', close: '关闭',
  },
  en: {
    title: 'New version available', required: 'You must install this update before continuing to use SparrowShell.', version: 'New version', policy: 'Update policy', published: 'Published',
    force: 'Required update', optional: 'Optional update', silent: 'Do not notify', notes: 'What’s new', emptyNotes: 'No release notes provided.',
    downloadHint: 'Select “Update now” to download and install in the app. SparrowShell restarts automatically when done.',
    downloading: 'Downloading update…', installing: 'Download complete. Installing, the app will restart automatically…',
    confirm: 'Update now', cancel: 'Cancel', working: 'Updating…', close: 'Close',
  },
} as const;

interface UpdateDialogProps {
  update: UpdateCheck;
  locale: Locale;
  manual: boolean;
  opening: boolean;
  installing: boolean;
  progress: UpdateDownloadProgress | null;
  error: string;
  onCancel: () => void;
  onConfirm: () => void;
}

export function UpdateDialog({ update, locale, manual, opening, installing, progress, error, onCancel, onConfirm }: UpdateDialogProps) {
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
  const percent = updateProgressPercent(progress);
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
        {opening ? (
          <div className="update-progress" role="status">
            <div className="update-progress-track">
              <div className={percent === null ? 'update-progress-bar indeterminate' : 'update-progress-bar'} style={percent === null ? undefined : { width: `${percent}%` }} />
            </div>
            <p className="update-progress-text">{installing ? t.installing : percent === null ? t.downloading : `${t.downloading} ${percent}%`}</p>
          </div>
        ) : (
          <p className="update-download-hint">{t.downloadHint}</p>
        )}
        {error && <p className="error" role="alert">{error}</p>}
        <footer><>{dismissible && <button className="button secondary" disabled={opening} onClick={onCancel}>{t.cancel}</button>}</><button autoFocus className="button primary" disabled={opening} onClick={onConfirm}><Download size={14} />{opening ? t.working : t.confirm}</button></footer>
      </div>
    </section>
  </div>;
}
