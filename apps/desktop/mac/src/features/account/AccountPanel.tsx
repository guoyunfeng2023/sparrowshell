import { useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { AlertTriangle, Cloud, KeyRound, LogOut, Pencil, RefreshCw, ShieldCheck, Trash2, X } from 'lucide-react';
import { desktop, isDesktop } from '../../ipc/client';
import type { Locale } from '../../i18n';
import type { AccountAuthResult, AccountStatus, AccountSwitchChoice, SyncConflict, SyncStatus } from '../../types';
import { LegalDialog } from '../support/LegalDialog';
import type { LegalDocument } from '../support/LegalDialog';
import { isValidAccountPassword, isValidNickname, isValidVerificationCode } from './accountValidation';
import { localizeAccountError } from './accountErrors';

type AuthMode = 'login' | 'register' | 'reset';

const copy = {
  zh: {
    title: '账号与云同步', signedOut: '未登录', signedIn: '已登录', expired: '登录已过期', local: '本地数据始终保留；密码、私钥口令和服务器凭据永不同步。',
    email: '邮箱', username: '用户名', password: '账号密码', currentPassword: '当前密码', newPassword: '新密码', code: '邮箱验证码', sendCode: '发送验证码', resend: '重新发送',
    login: '登录并启用同步', register: '注册并启用同步', resetPassword: '找回密码', resetSubmit: '重置密码', logging: '正在验证…', saving: '正在提交…',
    createAccount: '没有账号？立即注册', useLogin: '返回登录', forgotPassword: '忘记密码？',
    passwordHint: '密码至少 8 位，必须包含字母，并同时包含数字或符号；不能包含空格或中文。', resetHint: '通过邮箱验证码重置密码。重置后，所有设备都需要重新登录。',
    codeHint: '验证码 5 分钟内有效，60 秒后可重新发送。', codeSent: '验证码已发送，请查收邮件；5 分钟内有效。', resetSuccess: '密码已重置，请使用邮箱和新密码登录。',
    sync: '立即同步', syncing: '正在同步…', outbox: '待同步', syncedFolders: '同步文件夹', syncedServers: '同步服务器', conflicts: '待处理冲突', logout: '退出登录',
    remotePending: '本机已退出；云端撤销未确认，请下次登录后检查设备。', synced: '同步完成', noSync: '登录后可同步非敏感配置。',
    switchTitle: '选择这个账号如何使用本机数据', switchTarget: '目标账号', switchCounts: '当前本机工作区', switchFolders: '个文件夹', switchServers: '台服务器', switchSnapshot: '覆盖或合并前会保留本机全量快照，原始数据不会被直接删除。',
    cloudReplace: '从云端覆盖本地', cloudReplaceHelp: '使用这个账号的云端配置替换当前展示内容；本机原数据先保存为快照。',
    localReplace: '本地覆盖云端', localReplaceHelp: '上传当前本机全部配置；云端独有项将删除，本机原数据先保存为快照。',
    merge: '本地云端合并', mergeHelp: '合并双方全部配置；同一项内容不同时保留为待处理冲突，不静默覆盖。',
    switchConfirmTitle: '确认同步方式', switchConfirmAction: '确认并同步', switchConfirmNotice: '请确认你已理解所选方式的影响；取消不会执行同步。',
    cloudReplaceWarning: '当前本机展示的配置将被该账号的云端配置替换。本机原始数据会先保存为快照，但替换后当前工作区将不再直接展示这些内容。',
    localReplaceWarning: '当前本机全部配置将上传到云端，云端独有的文件夹和服务器配置将被删除，其他设备后续同步也会受到影响。请确认本机配置就是你希望保留的版本。',
    mergeWarning: '本地和云端配置将合并。相同实体的内容不一致时会生成待处理冲突，需要你之后手动选择保留哪个版本。',
    switchApplying: '正在处理并同步…', cancelSwitch: '取消并退出该账号', switchPartial: '本地切换已完成，但首轮同步失败。数据仍保留在本机，可稍后点击“立即同步”。',
    profile: '账号资料', editUsername: '修改用户名', saveUsername: '保存用户名', changePassword: '修改密码', usernameHint: '用户名仅用于展示，登录仍使用邮箱；最多 30 个字符。',
    passwordChanged: '密码已修改，所有设备登录状态已撤销，请使用新密码重新登录。', usernameChanged: '用户名已更新。',
    conflictTitle: '同步冲突', conflictHelp: '本地与云端版本都会保留，选择前不会静默覆盖。', localVersion: '本地', remoteVersion: '云端', keepLocal: '保留本地并重新基于云端版本推送', keepRemote: '保留云端并放弃此实体的本地待同步修改', revision: '版本', deleted: '删除标记', confirmRemote: '确认放弃这个实体的本地待同步修改？其他本地数据不受影响。',
    delete: '注销云账号', deleteHelp: '云端注销会进入服务端删除流程。本机配置、历史和 outbox 不会随账号注销删除。', deleteConfirm: '输入 DELETE 确认注销云账号', close: '关闭', cancel: '取消', preview: '真实账号与同步功能请在 macOS 客户端中使用。',
    invalidEmail: '请输入有效的邮箱地址。', requiredPassword: '请输入账号密码。', requiredCurrentPassword: '请输入当前密码。', invalidCode: '请输入 6 位数字验证码。', invalidPassword: '密码至少 8 位，必须包含字母及数字或符号，且不能包含空格或中文。', invalidUsername: '用户名必须为 1～30 个字符。',
  },
  en: {
    title: 'Account & Cloud Sync', signedOut: 'Signed out', signedIn: 'Signed in', expired: 'Session expired', local: 'Local data always remains. Server credentials, private keys and passphrases are never synced.',
    email: 'Email', username: 'Username', password: 'Account password', currentPassword: 'Current password', newPassword: 'New password', code: 'Email verification code', sendCode: 'Send code', resend: 'Resend',
    login: 'Sign in and enable sync', register: 'Register and enable sync', resetPassword: 'Reset password', resetSubmit: 'Reset password', logging: 'Verifying…', saving: 'Submitting…',
    createAccount: 'No account? Register now', useLogin: 'Back to sign in', forgotPassword: 'Forgot password?',
    passwordHint: 'Use at least 8 characters with letters and digits or symbols. Spaces and non-ASCII characters are not allowed.', resetHint: 'Reset with an email verification code. All devices must sign in again afterwards.',
    codeHint: 'The code is valid for 5 minutes. You can resend after 60 seconds.', codeSent: 'Verification code sent. It is valid for 5 minutes.', resetSuccess: 'Password reset. Sign in with your email and new password.',
    sync: 'Sync now', syncing: 'Syncing…', outbox: 'Pending', syncedFolders: 'Synced folders', syncedServers: 'Synced servers', conflicts: 'Conflicts', logout: 'Sign out',
    remotePending: 'Signed out locally. Cloud revocation was not confirmed; inspect the device after your next sign-in.', synced: 'Synchronization completed', noSync: 'Sign in to sync non-sensitive configurations.',
    switchTitle: 'Choose how this account uses local data', switchTarget: 'Account', switchCounts: 'Current local workspace', switchFolders: 'folders', switchServers: 'servers', switchSnapshot: 'A complete local snapshot is retained before replacement or merge. Original data is not directly erased.',
    cloudReplace: 'Replace local with cloud', cloudReplaceHelp: 'Replace the visible workspace with this account’s cloud configuration after archiving the current local data.',
    localReplace: 'Replace cloud with local', localReplaceHelp: 'Upload all current local definitions and delete cloud-only entities. The local workspace is archived first.',
    merge: 'Merge local and cloud', mergeHelp: 'Merge all definitions. Different content for the same entity becomes a conflict instead of being overwritten silently.',
    switchConfirmTitle: 'Confirm synchronization', switchConfirmAction: 'Confirm and sync', switchConfirmNotice: 'Make sure you understand the impact of the selected operation. Canceling does not synchronize anything.',
    cloudReplaceWarning: 'The visible local configuration will be replaced with this account’s cloud configuration. The original local data is archived first, but it will no longer appear directly in the current workspace.',
    localReplaceWarning: 'All current local definitions will be uploaded. Cloud-only folders and server configurations will be deleted, affecting subsequent synchronization on other devices. Confirm that the local workspace is the version you want to keep.',
    mergeWarning: 'Local and cloud configurations will be merged. Different content for the same entity becomes a pending conflict that you must resolve manually later.',
    switchApplying: 'Applying and syncing…', cancelSwitch: 'Cancel and sign out', switchPartial: 'The local switch completed, but the first sync failed. Your data remains local; use “Sync now” later.',
    profile: 'Account profile', editUsername: 'Edit username', saveUsername: 'Save username', changePassword: 'Change password', usernameHint: 'For display only. Sign in still uses your email. Maximum 30 characters.',
    passwordChanged: 'Password changed. All device sessions were revoked; sign in with your new password.', usernameChanged: 'Username updated.',
    conflictTitle: 'Synchronization conflicts', conflictHelp: 'Both versions are preserved until you choose. Nothing is overwritten silently.', localVersion: 'Local', remoteVersion: 'Cloud', keepLocal: 'Keep local and rebase on the cloud revision', keepRemote: 'Keep cloud and discard pending local changes', revision: 'Revision', deleted: 'Tombstone', confirmRemote: 'Discard pending local changes for this entity only? Other local data is unaffected.',
    delete: 'Delete cloud account', deleteHelp: 'Cloud deletion starts a server-side deletion workflow. Local configurations, history and outbox are not deleted with the account.', deleteConfirm: 'Type DELETE to confirm cloud account deletion', close: 'Close', cancel: 'Cancel', preview: 'Use the macOS app for real account and synchronization features.',
    invalidEmail: 'Enter a valid email address.', requiredPassword: 'Enter your account password.', requiredCurrentPassword: 'Enter your current password.', invalidCode: 'Enter the 6-digit verification code.', invalidPassword: 'Use at least 8 characters with letters and digits or symbols, without spaces or non-ASCII characters.', invalidUsername: 'Username must contain 1–30 characters.',
  },
} as const;

export function AccountPanel({ locale, initialStatus, onClose, onCatalogChanged, onAccountChanged }: { locale: Locale; initialStatus: AccountStatus; onClose: () => void; onCatalogChanged: () => Promise<void>; onAccountChanged: () => Promise<void> }) {
  const t = copy[locale];
  const [status, setStatus] = useState<AccountStatus>(initialStatus);
  const [initializing, setInitializing] = useState(isDesktop && initialStatus.state !== 'signed_in');
  const [sync, setSync] = useState<SyncStatus>({ cursor: 0, outbox: 0, conflicts: 0 });
  const [conflicts, setConflicts] = useState<SyncConflict[]>([]);
  const [catalogCounts, setCatalogCounts] = useState({ folders: 0, servers: 0 });
  const [pendingSwitch, setPendingSwitch] = useState<AccountAuthResult | null>(null);
  const [switchConfirmation, setSwitchConfirmation] = useState<AccountSwitchChoice | null>(null);
  const [mode, setMode] = useState<AuthMode>('login');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [code, setCode] = useState('');
  const [nickname, setNickname] = useState('');
  const [editingNickname, setEditingNickname] = useState(false);
  const [changingPassword, setChangingPassword] = useState(false);
  const [confirmation, setConfirmation] = useState('');
  const [deleting, setDeleting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [cooldown, setCooldown] = useState(0);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [legal, setLegal] = useState<LegalDocument | null>(null);

  const load = async (fetchProfile = false, knownStatus?: AccountStatus) => {
    if (!isDesktop) return;
    let nextStatus = knownStatus ?? await desktop.accountStatus();
    const [nextSync, nextConflicts, nextPendingSwitch, nextCatalog] = await Promise.all([desktop.syncStatus(), desktop.syncConflicts(), desktop.accountPendingSwitch(), desktop.catalog()]);
    if (fetchProfile && 'session' in nextStatus) {
      try { nextStatus = { state: 'signed_in', session: await desktop.accountProfile() }; } catch { /* Keep the local state when offline or when reauthentication is required. */ }
    }
    setStatus(nextStatus); setSync(nextSync); setConflicts(nextConflicts); setPendingSwitch(nextPendingSwitch);
    setCatalogCounts({ folders: nextCatalog.folders.length, servers: nextCatalog.servers.length });
    if (!knownStatus) await onAccountChanged();
  };
  useEffect(() => {
    void load(true, initialStatus)
      .catch((reason) => setError(String(reason)))
      .finally(() => setInitializing(false));
  }, []);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    void desktop.onSync(() => {
      if (!disposed) {
        setError((current) => current.startsWith('自动同步失败：') || current.startsWith('Automatic sync failed:') ? '' : current);
        void Promise.all([desktop.syncStatus(), desktop.syncConflicts(), desktop.catalog()])
          .then(([nextSync, nextConflicts, nextCatalog]) => {
            if (!disposed) {
              setSync(nextSync); setConflicts(nextConflicts);
              setCatalogCounts({ folders: nextCatalog.folders.length, servers: nextCatalog.servers.length });
            }
          })
          .catch((reason) => setError(String(reason)));
      }
    }).then((dispose) => {
      if (disposed) dispose(); else unlisten = dispose;
    });
    void desktop.onSyncError((reason) => {
      if (!disposed) setError(locale === 'zh' ? `自动同步失败：${reason}` : `Automatic sync failed: ${reason}`);
    }).then((dispose) => {
      if (disposed) dispose(); else unlistenError = dispose;
    });
    return () => { disposed = true; unlisten?.(); unlistenError?.(); };
  }, []);
  useEffect(() => {
    if (cooldown <= 0) return;
    const timer = window.setInterval(() => setCooldown((value) => Math.max(0, value - 1)), 1000);
    return () => window.clearInterval(timer);
  }, [cooldown]);

  const run = async (operation: () => Promise<void>) => {
    setBusy(true); setError(''); setNotice('');
    try { await operation(); await load(); }
    catch (reason) { setError(localizeAccountError(reason, locale)); }
    finally { setBusy(false); setPassword(''); setCurrentPassword(''); setNewPassword(''); }
  };
  const changeMode = (next: AuthMode) => {
    setMode(next); setError(''); setNotice(''); setPassword(''); setCode(''); setCooldown(0);
  };
  const sendCode = async () => {
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim())) { setError(t.invalidEmail); return; }
    setBusy(true); setError(''); setNotice('');
    try {
      await desktop.accountSendVerificationCode(email.trim(), mode === 'register' ? 'register' : 'password_reset', locale);
      setCooldown(60); setNotice(t.codeSent);
    } catch (reason) { setError(localizeAccountError(reason, locale)); }
    finally { setBusy(false); }
  };
  const submitAuth = (event: FormEvent) => {
    event.preventDefault();
    setError(''); setNotice('');
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim())) { setError(t.invalidEmail); return; }
    if (mode === 'login' && !password) { setError(t.requiredPassword); return; }
    if (mode !== 'login' && !isValidVerificationCode(code)) { setError(t.invalidCode); return; }
    if (mode !== 'login' && !isValidAccountPassword(password)) { setError(t.invalidPassword); return; }
    void run(async () => {
      if (mode === 'register') await desktop.accountRegister(email.trim(), password, code);
      else if (mode === 'reset') {
        await desktop.accountResetPassword(email.trim(), code, password);
        changeMode('login'); setNotice(t.resetSuccess);
      } else await desktop.accountLogin(email.trim(), password);
    });
  };
  const applyAccountSwitch = (choice: AccountSwitchChoice) => {
    setSwitchConfirmation(null);
    void run(async () => {
      const result = await desktop.accountSwitch(choice);
      setPendingSwitch(null);
      await onCatalogChanged();
      if (result.syncError) setError(`${t.switchPartial} ${localizeAccountError(result.syncError, locale)}`);
      else setNotice(`${t.synced}: +${result.syncReport?.pushed ?? 0} / ↓${result.syncReport?.pulled ?? 0}`);
    });
  };
  const cancelAccountSwitch = () => {
    setSwitchConfirmation(null);
    void (async () => {
      setBusy(true); setError(''); setNotice('');
      try {
        await desktop.accountCancelSwitch();
        setPendingSwitch(null);
        setStatus({ state: 'signed_out' });
        setConflicts([]);
        await load();
      } catch (reason) { setError(localizeAccountError(reason, locale)); }
      finally { setBusy(false); setPassword(''); setCurrentPassword(''); setNewPassword(''); }
    })();
  };
  const switchOptions: { value: AccountSwitchChoice; label: string; help: string; warning: string }[] = [
    { value: 'cloud_replace_local', label: t.cloudReplace, help: t.cloudReplaceHelp, warning: t.cloudReplaceWarning },
    { value: 'local_replace_cloud', label: t.localReplace, help: t.localReplaceHelp, warning: t.localReplaceWarning },
    { value: 'merge', label: t.merge, help: t.mergeHelp, warning: t.mergeWarning },
  ];
  const confirmedSwitchOption = switchOptions.find((option) => option.value === switchConfirmation);
  const session = 'session' in status ? status.session : null;
  const codeForm = mode !== 'login' && <label>{t.code}<div className="verification-field"><input required inputMode="numeric" maxLength={6} autoComplete="one-time-code" value={code} onChange={(event) => setCode(event.target.value.replace(/\D/g, '').slice(0, 6))} /><button className="button secondary" type="button" disabled={busy || cooldown > 0 || !isDesktop} onClick={() => void sendCode()}>{cooldown > 0 ? `${cooldown}s` : code ? t.resend : t.sendCode}</button></div><small className="field-hint">{t.codeHint}</small></label>;

  return <div className="modal-backdrop"><section className="modal account-modal" role="dialog" aria-modal="true" aria-label={t.title}>
    <header><h2><Cloud size={17} />{t.title}</h2><button className="icon" onClick={onClose} aria-label={t.close}><X size={17} /></button></header>
    <div className="account-content">
      <p className="account-boundary"><ShieldCheck size={16} />{t.local}</p>
      {!isDesktop && <p className="muted">{t.preview}</p>}
      {pendingSwitch ? <section className="account-switch-choice">
        <div><h3>{t.switchTitle}</h3><p>{t.switchTarget}: <strong>{pendingSwitch.session.nickname || pendingSwitch.session.email || pendingSwitch.session.userId}</strong></p><p>{t.switchCounts}: <strong>{pendingSwitch.localFolders}</strong> {t.switchFolders} · <strong>{pendingSwitch.localServers}</strong> {t.switchServers}</p></div>
        <p className="account-switch-snapshot"><ShieldCheck size={15} />{t.switchSnapshot}</p>
        <div className="account-switch-options">{switchOptions.map((option) => <button key={option.value} type="button" disabled={busy || !isDesktop} onClick={() => { setError(''); setNotice(''); setSwitchConfirmation(option.value); }}><strong>{option.label}</strong><span>{option.help}</span></button>)}</div>
        <button className="button secondary" type="button" disabled={busy} onClick={cancelAccountSwitch}>{busy ? t.switchApplying : t.cancelSwitch}</button>
      </section> : initializing ? <div className="account-initial-loading" role="status"><RefreshCw size={18} className="spin" /><span>{locale === 'zh' ? '正在恢复账号状态…' : 'Restoring account…'}</span></div> : session ? <>
        <div className="account-summary">
          <div><span className={`status-dot ${status.state === 'signed_in' ? 'online' : ''}`} /><strong>{status.state === 'signed_in' ? t.signedIn : t.expired}</strong><small>{session.nickname || session.email || session.userId}</small></div>
          <div className="sync-stats"><span>{t.syncedFolders}<b>{catalogCounts.folders}</b></span><span>{t.syncedServers}<b>{catalogCounts.servers}</b></span><span>{t.outbox}<b>{sync.outbox}</b></span><span>{t.conflicts}<b>{sync.conflicts}</b></span></div>
          <div className="account-actions"><button className="button primary" disabled={busy} onClick={() => void run(async () => { const report = await desktop.syncNow(); setNotice(`${t.synced}: +${report.pushed} / ↓${report.pulled}`); await onCatalogChanged(); })}><RefreshCw size={14} />{busy ? t.syncing : t.sync}</button><button className="button secondary" disabled={busy} onClick={() => void run(async () => { const result = await desktop.accountLogout(); if (!result.remoteRevoked) setNotice(t.remotePending); })}><LogOut size={14} />{t.logout}</button></div>
        </div>
        <section className="account-profile">
          <h3>{t.profile}</h3>
          <dl><div><dt>{t.email}</dt><dd>{session.email || '—'}</dd></div><div><dt>{t.username}</dt><dd>{session.nickname || '—'}</dd></div></dl>
          <div className="account-profile-actions"><button className="button secondary" type="button" onClick={() => { setEditingNickname((value) => !value); setChangingPassword(false); setNickname(session.nickname || ''); }}><Pencil size={13} />{t.editUsername}</button><button className="button secondary" type="button" onClick={() => { setChangingPassword((value) => !value); setEditingNickname(false); }}><KeyRound size={13} />{t.changePassword}</button></div>
          {editingNickname && <form className="account-inline-form" onSubmit={(event) => { event.preventDefault(); if (!isValidNickname(nickname)) { setError(t.invalidUsername); return; } void run(async () => { await desktop.accountUpdateNickname(nickname.trim()); setEditingNickname(false); setNotice(t.usernameChanged); }); }}><label>{t.username}<input autoFocus maxLength={30} autoCapitalize="none" autoCorrect="off" spellCheck={false} value={nickname} onChange={(event) => setNickname(event.target.value)} /></label><small>{t.usernameHint}</small><div><button className="button secondary" type="button" onClick={() => setEditingNickname(false)}>{t.cancel}</button><button className="button primary" disabled={busy}>{t.saveUsername}</button></div></form>}
          {changingPassword && <form className="account-inline-form" noValidate onSubmit={(event) => { event.preventDefault(); setError(''); setNotice(''); if (!currentPassword) { setError(t.requiredCurrentPassword); return; } if (!isValidAccountPassword(newPassword)) { setError(t.invalidPassword); return; } void run(async () => { await desktop.accountChangePassword(currentPassword, newPassword); setChangingPassword(false); setNotice(t.passwordChanged); }); }}><label>{t.currentPassword}<input required type="password" autoComplete="current-password" value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} /></label><label>{t.newPassword}<input required type="password" autoComplete="new-password" value={newPassword} onChange={(event) => setNewPassword(event.target.value)} /></label><small>{t.passwordHint}</small><div><button className="button secondary" type="button" onClick={() => setChangingPassword(false)}>{t.cancel}</button><button className="button primary" disabled={busy}>{busy ? t.saving : t.changePassword}</button></div></form>}
        </section>
      </> : <form className="account-login" noValidate onSubmit={submitAuth}>
        <h3>{mode === 'register' ? t.register : mode === 'reset' ? t.resetPassword : t.signedOut}</h3><p className="muted">{mode === 'login' ? t.noSync : mode === 'reset' ? t.resetHint : t.passwordHint}</p>
        <label>{t.email}<input required type="email" autoComplete="username" autoCapitalize="none" autoCorrect="off" spellCheck={false} value={email} onChange={(event) => setEmail(event.target.value)} /></label>
        {codeForm}
        <label>{mode === 'reset' ? t.newPassword : t.password}<input required type="password" autoComplete={mode === 'login' ? 'current-password' : 'new-password'} value={password} onChange={(event) => setPassword(event.target.value)} /></label>
        {mode !== 'login' && <p className="password-rule">{t.passwordHint}</p>}
        <button className="button primary" disabled={busy || !isDesktop}>{busy ? t.logging : mode === 'register' ? t.register : mode === 'reset' ? t.resetSubmit : t.login}</button>
        <div className="account-switch-links">{mode === 'login' ? <><button type="button" onClick={() => changeMode('register')}>{t.createAccount}</button><button type="button" onClick={() => changeMode('reset')}>{t.forgotPassword}</button></> : <button type="button" onClick={() => changeMode('login')}>{t.useLogin}</button>}</div>
      </form>}
      {notice && <p className="account-notice" role="status">{notice}</p>}
      {error && <p className="error" role="alert">{error}</p>}
      {!pendingSwitch && session && conflicts.length > 0 && <section className="conflict-section"><h3>{t.conflictTitle}<span>{conflicts.length}</span></h3><p className="muted">{t.conflictHelp}</p>{conflicts.map((conflict) => {
        const localName = typeof conflict.localPayload.name === 'string' ? conflict.localPayload.name : conflict.entityId;
        const remoteName = typeof conflict.remotePayload.data?.name === 'string' ? conflict.remotePayload.data.name : conflict.entityId;
        return <article className="conflict-card" key={conflict.conflictId}><div className="conflict-versions"><div><small>{t.localVersion}</small><strong>{localName}</strong></div><div><small>{t.remoteVersion} · {t.revision} {conflict.remoteRevision}{conflict.remotePayload.deleted ? ` · ${t.deleted}` : ''}</small><strong>{remoteName}</strong></div></div><button className="button secondary" disabled={busy} onClick={() => void run(async () => { await desktop.resolveSyncConflict(conflict.conflictId, true); })}>{t.keepLocal}</button><button className="button danger" disabled={busy} onClick={() => { if (window.confirm(t.confirmRemote)) void run(async () => { await desktop.resolveSyncConflict(conflict.conflictId, false); await onCatalogChanged(); }); }}>{t.keepRemote}</button></article>;
      })}</section>}
      {!pendingSwitch && session && <section className="account-danger"><button className="danger-link" onClick={() => setDeleting((value) => !value)}><Trash2 size={14} />{t.delete}</button>{deleting && <div><p><AlertTriangle size={14} />{t.deleteHelp}</p><label>{t.deleteConfirm}<input autoCapitalize="none" autoCorrect="off" spellCheck={false} value={confirmation} onChange={(event) => setConfirmation(event.target.value)} autoComplete="off" /></label><button className="button danger" disabled={busy || confirmation !== 'DELETE'} onClick={() => void run(async () => { await desktop.accountDelete(confirmation); })}>{t.delete}</button></div>}</section>}
      <footer className="account-legal legal-links"><button type="button" onClick={() => setLegal('terms')}>{locale === 'zh' ? '服务条款' : 'Terms of Service'}</button><span>·</span><button type="button" onClick={() => setLegal('privacy')}>{locale === 'zh' ? '隐私政策' : 'Privacy Policy'}</button></footer>
    </div>
    {legal && <LegalDialog locale={locale} document={legal} onClose={() => setLegal(null)} />}
    {confirmedSwitchOption && <div className="modal-backdrop account-switch-confirm-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) setSwitchConfirmation(null); }}>
      <section className="modal account-switch-confirm-modal" role="alertdialog" aria-modal="true" aria-labelledby="account-switch-confirm-title" aria-describedby="account-switch-confirm-description">
        <header><h2 id="account-switch-confirm-title"><AlertTriangle size={18} />{t.switchConfirmTitle}</h2><button className="icon" type="button" disabled={busy} onClick={() => setSwitchConfirmation(null)} aria-label={t.close}><X size={17} /></button></header>
        <div className="account-switch-confirm-content">
          <strong>{confirmedSwitchOption.label}</strong>
          <p id="account-switch-confirm-description">{confirmedSwitchOption.warning}</p>
          <small>{t.switchConfirmNotice}</small>
          <footer><button className="button secondary" type="button" autoFocus disabled={busy} onClick={() => setSwitchConfirmation(null)}>{t.cancel}</button><button className={`button ${confirmedSwitchOption.value === 'local_replace_cloud' ? 'danger' : 'primary'}`} type="button" disabled={busy} onClick={() => applyAccountSwitch(confirmedSwitchOption.value)}>{busy ? t.switchApplying : t.switchConfirmAction}</button></footer>
        </div>
      </section>
    </div>}
  </section></div>;
}
