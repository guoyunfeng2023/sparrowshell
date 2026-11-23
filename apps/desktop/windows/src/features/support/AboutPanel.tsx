import { useState } from 'react';
import type { FormEvent } from 'react';
import { CheckCircle2, Info, Paperclip, RefreshCw, Send, ShieldCheck, X } from 'lucide-react';
import { desktop, isDesktop } from '../../ipc/client';
import type { Locale } from '../../i18n';
import type { UpdateCheck } from '../../types';
import { LegalDialog } from './LegalDialog';
import type { LegalDocument } from './LegalDialog';
import packageJson from '../../../package.json';

const APP_VERSION = packageJson.version;
const copy = {
  zh: {
    about: '关于 SparrowShell', tagline: '本地优先的智能运维工作台', version: '版本', platform: 'Windows · x64', check: '检查更新', checking: '正在检查…', latest: '当前已是最新版本', available: '发现新版本', force: '强制更新', optional: '可选择更新', download: '通过浏览器下载', updateFailed: '暂时无法检查更新，请稍后重试。', feedback: '问题反馈', feedbackHint: '无需登录即可反馈；登录后会安全关联当前账号。', description: '问题描述', descriptionPlaceholder: '请描述遇到的问题、操作步骤和实际结果…', attachment: '截图或附件（选填）', choose: '选择文件', attachmentHint: '支持图片、PDF、TXT、LOG、ZIP，单个文件不超过 10 MB。', contact: '联系方式（选填）', contactPlaceholder: '邮箱或其他便于联系你的方式', submit: '提交反馈', submitting: '正在提交…', success: '反馈已提交', feedbackId: '反馈编号', another: '继续反馈其他问题', back: '返回关于', privacyNote: '仅提交你填写的内容和主动选择的附件，不会自动附带终端、服务器或 AI 数据。', terms: '服务条款', privacy: '隐私政策', preview: '真实检查更新与反馈功能请在 Windows 客户端中使用。', submitFailed: '反馈提交失败，请检查服务状态后重试；已填写内容会保留。', unauthorized: '登录状态已失效，请重新登录，或退出登录后匿名提交。', rateLimited: '提交过于频繁，请稍后再试。',
  },
  en: {
    about: 'About SparrowShell', tagline: 'Local-first intelligent operations workspace', version: 'Version', platform: 'Windows · x64', check: 'Check for updates', checking: 'Checking…', latest: 'You are on the latest version', available: 'New version available', force: 'Required update', optional: 'Optional update', download: 'Download in browser', updateFailed: 'Updates cannot be checked right now. Please try again later.', feedback: 'Feedback', feedbackHint: 'No sign-in required. Signed-in submissions are securely linked to your account.', description: 'Problem description', descriptionPlaceholder: 'Describe the issue, steps taken and actual result…', attachment: 'Screenshot or attachment (optional)', choose: 'Choose file', attachmentHint: 'Images, PDF, TXT, LOG or ZIP; one file up to 10 MB.', contact: 'Contact (optional)', contactPlaceholder: 'Email or another way to reach you', submit: 'Submit feedback', submitting: 'Submitting…', success: 'Feedback submitted', feedbackId: 'Feedback ID', another: 'Submit another issue', back: 'Back to About', privacyNote: 'Only your entries and explicitly selected attachment are sent. Terminal, server and AI data are never attached automatically.', terms: 'Terms of Service', privacy: 'Privacy Policy', preview: 'Use the Windows app for real update checks and feedback.', submitFailed: 'Feedback could not be submitted. Check the service and try again; your entries are preserved.', unauthorized: 'Your session is invalid. Sign in again, or sign out and submit anonymously.', rateLimited: 'Too many submissions. Please try again later.',
  },
} as const;

export function AboutPanel({ locale, onCheckUpdate }: { locale: Locale; onCheckUpdate?: () => Promise<UpdateCheck> }) {
  const t = copy[locale];
  const [page, setPage] = useState<'about' | 'feedback'>('about');
  const [legal, setLegal] = useState<LegalDocument | null>(null);
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<UpdateCheck | null>(null);
  const [updateError, setUpdateError] = useState('');
  const [description, setDescription] = useState('');
  const [contact, setContact] = useState('');
  const [attachment, setAttachment] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [feedbackError, setFeedbackError] = useState('');
  const [receipt, setReceipt] = useState<string | null>(null);

  const check = async () => {
    setChecking(true); setUpdateError(''); setUpdate(null);
    try { setUpdate(await (onCheckUpdate ? onCheckUpdate() : desktop.checkUpdate(true))); }
    catch { setUpdateError(t.updateFailed); }
    finally { setChecking(false); }
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault(); setSubmitting(true); setFeedbackError(''); setReceipt(null);
    try {
      const result = await desktop.submitFeedback(description, contact, attachment);
      setReceipt(result.feedbackId);
    } catch (reason) {
      const message = String(reason);
      setFeedbackError(message.includes('unauthorized') ? t.unauthorized : message.includes('rate_limited') ? t.rateLimited : message.includes('附件') ? message : t.submitFailed);
    } finally { setSubmitting(false); }
  };
  const legalLinks = <div className="legal-links"><button onClick={() => setLegal('terms')}>{t.terms}</button><span>·</span><button onClick={() => setLegal('privacy')}>{t.privacy}</button></div>;

  return <div className="about-panel">
    {page === 'about' ? <>
      <div className="about-hero"><span className="about-logo"><Info size={24} /></span><div><h3>SparrowShell</h3><p>{t.tagline}</p></div></div>
      <dl className="about-facts"><div><dt>{t.version}</dt><dd>v{APP_VERSION}</dd></div><div><dt>Platform</dt><dd>{t.platform}</dd></div></dl>
      <div className="about-actions"><button className="button secondary" disabled={checking || !isDesktop} onClick={() => void check()}><RefreshCw size={14} className={checking ? 'spin' : ''} />{checking ? t.checking : t.check}</button><button className="button primary" disabled={!isDesktop} onClick={() => setPage('feedback')}><Send size={14} />{t.feedback}</button></div>
      {!isDesktop && <p className="muted">{t.preview}</p>}
      {updateError && <p className="error" role="alert">{updateError}</p>}
      {update?.status === 'no_update' && <p className="about-update-status" role="status"><CheckCircle2 size={15} />{t.latest} · v{APP_VERSION}</p>}
      <div className="about-support"><p>{t.feedbackHint}</p>{legalLinks}</div>
    </> : <>
      <div className="feedback-heading"><div><h3>{t.feedback}</h3><p>{t.feedbackHint}</p></div><button className="button secondary" onClick={() => setPage('about')}>{t.back}</button></div>
      {receipt ? <div className="feedback-success" role="status"><CheckCircle2 size={28} /><h3>{t.success}</h3><p>{t.feedbackId}</p><code>{receipt}</code><button className="button secondary" onClick={() => { setReceipt(null); setDescription(''); setContact(''); setAttachment(null); }}>{t.another}</button></div> : <form className="feedback-form" onSubmit={submit}>
        <label>{t.description}<textarea required maxLength={10000} rows={5} autoCapitalize="none" autoCorrect="off" spellCheck={false} value={description} onChange={(event) => setDescription(event.target.value)} placeholder={t.descriptionPlaceholder} /></label>
        <label>{t.attachment}<div className="feedback-attachment"><button type="button" className="button secondary" disabled={!isDesktop} onClick={() => void desktop.selectLocalFile().then(setAttachment)}><Paperclip size={14} />{t.choose}</button>{attachment && <span title={attachment}>{attachment.split(/[\\/]/).pop()}<button type="button" className="icon small" onClick={() => setAttachment(null)} aria-label="Remove"><X size={12} /></button></span>}</div><small>{t.attachmentHint}</small></label>
        <label>{t.contact}<input maxLength={320} autoCapitalize="none" autoCorrect="off" spellCheck={false} value={contact} onChange={(event) => setContact(event.target.value)} placeholder={t.contactPlaceholder} /></label>
        <p className="feedback-privacy"><ShieldCheck size={14} />{t.privacyNote}</p>
        {feedbackError && <p className="error" role="alert">{feedbackError}</p>}
        <button className="button primary" disabled={submitting || !description.trim() || !isDesktop}><Send size={14} />{submitting ? t.submitting : t.submit}</button>
      </form>}
    </>}
    {legal && <LegalDialog locale={locale} document={legal} onClose={() => setLegal(null)} />}
  </div>;
}
