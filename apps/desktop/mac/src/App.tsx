import { useEffect, useMemo, useRef, useState } from 'react';
import type { FormEvent, PointerEvent, ReactNode } from 'react';
import { Bell, ChevronDown, ChevronRight, Copy, Folder, FolderPlus, Globe2, Languages, Moon, MoreHorizontal, PanelLeftClose, PanelRightClose, Play, Plus, RefreshCw, Search, Server, Settings2, ShieldCheck, Sun, TerminalSquare, Trash2, X, XCircle } from 'lucide-react';
import { desktop, isDesktop } from './ipc/client';
import { messages } from './i18n';
import type { MessageKey } from './i18n';
import { Terminal } from './features/terminal/Terminal';
import { TerminalFontSizeInput, terminalFonts } from './features/terminal/terminalOptions';
import { AiPanel } from './features/ai/AiPanel';
import { AccountPanel } from './features/account/AccountPanel';
import { HistoryPanel } from './features/history/HistoryPanel';
import { MonitorPanel } from './features/workbench/MonitorPanel';
import { FilesPanel } from './features/workbench/FilesPanel';
import { LocalizedContextMenu } from './features/workbench/LocalizedContextMenu';
import { clampAiPanelHeight, draggedAiPanelHeight, MIN_AI_PANEL_HEIGHT } from './features/workbench/panelSizing';
import { DEFAULT_DYNAMIC_AGENT_STEPS, MAX_DYNAMIC_AGENT_STEPS, MIN_DYNAMIC_AGENT_STEPS } from './features/ai/dynamicAiAgent';
import { AutomationPanel } from './features/automation/AutomationPanel';
import { ConnectionFeedback, isConnectingState } from './features/connection/ConnectionFeedback';
import { AboutPanel } from './features/support/AboutPanel';
import { UpdateDialog } from './features/support/UpdateDialog';
import { shouldShowUpdatePrompt } from './features/support/updateFlow';
import { folderOrderChanges, serverOrderChanges } from './features/catalog/catalogOrdering';
import type { CatalogDropPosition, CatalogDropTarget } from './features/catalog/catalogOrdering';
import { folderDeletionImpact } from './features/catalog/catalogDeletion';
import type { AccountStatus, AiContextData, CatalogSnapshot, ConnectResult, Folder as FolderRecord, HostKeyChallenge, Preferences, ServerInput, ServerRecord, SessionSnapshot, TargetRef, UpdateCheck } from './types';

const emptyServer: ServerInput = { id: null, folderId: null, name: '', host: '', port: 22, username: '', authType: 'password', privateKeyPathHint: null, jumpServerId: null };
const defaults: Preferences = { theme: 'dark', locale: 'zh', terminalFontFamily: 'SFMono-Regular, Menlo, monospace', terminalFontSize: 13, terminalCompletionEnabled: true, aiMaxSteps: DEFAULT_DYNAMIC_AGENT_STEPS };
const preferencesStorageKey = import.meta.env.DEV ? 'sparrowshell.desktop.development.preferences' : 'sparrowshell.desktop.preferences';

function Modal({ title, children, onClose }: { title: string; children: ReactNode; onClose: () => void }) {
  return <div className="modal-backdrop"><section className="modal" role="dialog" aria-modal="true" aria-label={title}>
    <header><h2>{title}</h2><button className="icon" onClick={onClose} aria-label="Close"><X size={17} /></button></header>{children}
  </section></div>;
}

function ServerForm({ server, initialFolderId, catalog, t, onClose, onSaved }: { server: ServerRecord | null; initialFolderId: string | null; catalog: CatalogSnapshot; t: (key: MessageKey) => string; onClose: () => void; onSaved: () => Promise<void> }) {
  const [form, setForm] = useState<ServerInput>(server ? { id: server.id, folderId: server.folderId, name: server.name, host: server.host, port: server.port, username: server.username, authType: server.authType, privateKeyPathHint: server.privateKeyPathHint, jumpServerId: server.jumpServerId } : { ...emptyServer, folderId: initialFolderId });
  const [secret, setSecret] = useState('');
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testSuccess, setTestSuccess] = useState(false);
  const [testChallenge, setTestChallenge] = useState<HostKeyChallenge | null>(null);
  const [error, setError] = useState('');
  const clearTestResult = () => { setTestSuccess(false); setTestChallenge(null); setError(''); };
  const update = <K extends keyof ServerInput>(key: K, value: ServerInput[K]) => { clearTestResult(); setForm((current) => ({ ...current, [key]: value })); };
  const testConnection = async (acceptedHostKey: HostKeyChallenge | null = null) => {
    setTesting(true); setError(''); setTestSuccess(false);
    if (!acceptedHostKey) setTestChallenge(null);
    try {
      const result = await desktop.testConnection(form, secret || null, acceptedHostKey);
      setTestChallenge(result.hostKeyChallenge);
      setTestSuccess(!result.hostKeyChallenge);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setTesting(false); }
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError('');
    try {
      const saved = await desktop.saveServer(form);
      if (secret) await desktop.savePassword(saved.id, secret);
      await onSaved(); onClose();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); } finally { setBusy(false); }
  };
  return <Modal title={server ? t('edit') : t('newServer')} onClose={onClose}><form onSubmit={submit} className="form-grid">
    <label>{t('name')}<input required maxLength={128} autoCapitalize="none" autoCorrect="off" spellCheck={false} value={form.name} onChange={(e) => update('name', e.target.value)} placeholder="production-web-01" /></label>
    <label>{t('folder')}<select value={form.folderId ?? ''} onChange={(e) => update('folderId', e.target.value || null)}><option value="">{t('root')}</option>{catalog.folders.map((folder) => <option key={folder.id} value={folder.id}>{folder.name}</option>)}</select></label>
    <label className="span-2">{t('host')}<input required autoCapitalize="none" autoCorrect="off" spellCheck={false} value={form.host} onChange={(e) => update('host', e.target.value)} placeholder="192.0.2.10 / server.example.com" /></label>
    <label>{t('port')}<input required type="number" min={1} max={65535} value={form.port} onChange={(e) => update('port', Number(e.target.value))} /></label>
    <label>{t('username')}<input required autoCapitalize="none" autoCorrect="off" spellCheck={false} value={form.username} onChange={(e) => update('username', e.target.value)} autoComplete="off" /></label>
    <label>{t('authentication')}<select value={form.authType} onChange={(e) => update('authType', e.target.value as ServerInput['authType'])}><option value="password">{t('password')}</option><option value="private_key">{t('privateKey')}</option></select></label>
    <label>{t('jump')}<select value={form.jumpServerId ?? ''} onChange={(e) => update('jumpServerId', e.target.value || null)}><option value="">{t('none')}</option>{catalog.servers.filter((item) => item.id !== form.id).map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
    {form.authType === 'private_key' && <label className="span-2">{t('privateKeyPath')}<input required autoCapitalize="none" autoCorrect="off" spellCheck={false} value={form.privateKeyPathHint ?? ''} onChange={(e) => update('privateKeyPathHint', e.target.value)} placeholder="~/.ssh/id_ed25519" /></label>}
    <label className="span-2">{t(form.authType === 'password' ? 'password' : 'passphrase')}<input type="password" value={secret} onChange={(e) => { clearTestResult(); setSecret(e.target.value); }} placeholder={server ? t('passwordHint') : form.authType === 'private_key' ? t('passphraseHint') : ''} autoComplete="new-password" /></label>
    <p className="secret-note span-2"><ShieldCheck size={15} />{t('secret')}</p>
    {testChallenge && <div className="test-host-key span-2"><p>{t(testChallenge.previousFingerprint ? 'hostKeyChanged' : 'hostKeyNew')}</p><small>{testChallenge.host}:{testChallenge.port} · {testChallenge.algorithm}</small><code>{testChallenge.fingerprint}</code><button type="button" className="button secondary" disabled={testing} onClick={() => void testConnection(testChallenge)}>{t('trustAndTest')}</button></div>}
    {testSuccess && <p className="test-success span-2" role="status">{t('testConnectionSuccess')}</p>}
    {error && <p className="error span-2" role="alert">{error}</p>}
    <footer className="span-2 server-form-footer"><button type="button" className="button secondary" disabled={busy || testing || !isDesktop} onClick={() => void testConnection()}><Server size={14} />{testing ? t('testingConnection') : t('testConnection')}</button><div><button type="button" className="button secondary" disabled={testing} onClick={onClose}>{t('cancel')}</button><button className="button primary" disabled={busy || testing}>{busy ? '…' : t('save')}</button></div></footer>
  </form></Modal>;
}

interface TerminalTab {
  id: string;
  serverId: string;
  session: SessionSnapshot | null;
  hostKey: HostKeyChallenge | null;
  busy: boolean;
}

interface CatalogDragItem {
  kind: 'folder' | 'server';
  id: string;
}

interface CatalogDropIndicator {
  id: string;
  position: CatalogDropPosition;
}

interface CatalogPointerDrag {
  item: CatalogDragItem;
  pointerId: number;
  startX: number;
  startY: number;
  active: boolean;
}

interface CatalogDragPreview extends CatalogDragItem {
  name: string;
  x: number;
  y: number;
}

const MAX_TERMINAL_TABS = 8;

function mergeSession(tab: TerminalTab, next: SessionSnapshot): TerminalTab {
  if (tab.session && next.target.generation < tab.session.target.generation) return tab;
  return { ...tab, busy: isConnectingState(next.state), session: { ...next, channelId: next.channelId ?? tab.session?.channelId ?? null } };
}

export default function App() {
  const [preferences, setPreferences] = useState<Preferences>(() => {
    try { return { ...defaults, ...JSON.parse(localStorage.getItem(preferencesStorageKey) ?? '{}') }; } catch { return defaults; }
  });
  const [catalog, setCatalog] = useState<CatalogSnapshot>({ folders: [], servers: [] });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [leftVisible, setLeftVisible] = useState(true);
  const [rightVisible, setRightVisible] = useState(true);
  const [aiPanelHeight, setAiPanelHeight] = useState(320);
  const [aiPanelResizing, setAiPanelResizing] = useState(false);
  const centerPanel = useRef<HTMLElement>(null);
  const aiPanelDrag = useRef<{ pointerId: number; startY: number; startHeight: number } | null>(null);
  const [modal, setModal] = useState<'server' | 'folder' | 'settings' | 'account' | 'automation' | null>(null);
  const [settingsTab, setSettingsTab] = useState<'appearance' | 'about'>('appearance');
  const [editing, setEditing] = useState<ServerRecord | null>(null);
  const [folderParent, setFolderParent] = useState<string | null>(null);
  const [folderEditing, setFolderEditing] = useState<FolderRecord | null>(null);
  const [folderName, setFolderName] = useState('');
  const [folderMenu, setFolderMenu] = useState<string | null>(null);
  const [serverMenu, setServerMenu] = useState<string | null>(null);
  const [catalogMenu, setCatalogMenu] = useState<{ x: number; y: number } | null>(null);
  const [catalogDeleteTarget, setCatalogDeleteTarget] = useState<{ kind: 'server'; server: ServerRecord } | { kind: 'folder'; folder: FolderRecord; subfolderCount: number; serverCount: number } | null>(null);
  const [catalogDeleting, setCatalogDeleting] = useState(false);
  const draggedCatalogItem = useRef<CatalogDragItem | null>(null);
  const catalogPointerDrag = useRef<CatalogPointerDrag | null>(null);
  const catalogDropDestination = useRef<CatalogDropTarget | null>(null);
  const suppressCatalogClick = useRef(false);
  const [dropIndicator, setDropIndicator] = useState<CatalogDropIndicator | null>(null);
  const [catalogDragPreview, setCatalogDragPreview] = useState<CatalogDragPreview | null>(null);
  const [catalogSaving, setCatalogSaving] = useState(false);
  const [tabs, setTabs] = useState<TerminalTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [error, setError] = useState('');
  const [accountStatus, setAccountStatus] = useState<AccountStatus>({ state: 'signed_out' });
  const [catalogSyncing, setCatalogSyncing] = useState(false);
  const [aiContexts, setAiContexts] = useState<Record<string, AiContextData>>({});
  const [updatePrompt, setUpdatePrompt] = useState<{ update: UpdateCheck; manual: boolean; opening: boolean; downloadOpened: boolean; error: string } | null>(null);
  const updateCheckStarted = useRef(false);
  const updateAiContext = (target: TargetRef, patch: AiContextData) => {
    const key = `${target.serverId}:${target.sessionId}:${target.generation}`;
    setAiContexts((current) => ({ ...current, [key]: { ...current[key], ...patch } }));
  };
  const t = (key: MessageKey) => messages[preferences.locale][key];
  const selected = catalog.servers.find((server) => server.id === selectedId) ?? null;
  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? null;
  const activeServer = activeTab ? catalog.servers.find((server) => server.id === activeTab.serverId) ?? null : selected;
  const session = activeTab?.session ?? null;
  const accountSession = 'session' in accountStatus ? accountStatus.session : null;
  const connected = session?.state === 'connected';
  const busy = activeTab?.busy ?? false;
  const hostKey = activeTab?.hostKey ?? null;

  const refresh = async () => {
    const next = await desktop.catalog();
    setCatalog(next);
    setSelectedId((current) => current && next.servers.some((item) => item.id === current) ? current : next.servers[0]?.id ?? null);
  };
  const refreshAccountStatus = async () => setAccountStatus(await desktop.accountStatus());
  const checkForUpdates = async (manual: boolean): Promise<UpdateCheck> => {
    const update = await desktop.checkUpdate(manual);
    if (shouldShowUpdatePrompt(update, manual)) setUpdatePrompt({ update, manual, opening: false, downloadOpened: false, error: '' });
    return update;
  };
  const confirmUpdate = async () => {
    const prompt = updatePrompt;
    if (!prompt || prompt.opening) return;
    setUpdatePrompt({ ...prompt, opening: true, error: '' });
    try {
      if (!prompt.update.artifactUrl) throw new Error(preferences.locale === 'zh' ? '更新下载地址无效，请联系管理员。' : 'The update download URL is invalid. Contact your administrator.');
      await desktop.openExternal(prompt.update.artifactUrl);
      if (prompt.manual || prompt.update.updatePolicy !== 'force') setUpdatePrompt(null);
      else setUpdatePrompt((current) => current ? { ...current, opening: false, downloadOpened: true, error: '' } : null);
    } catch (reason) {
      setUpdatePrompt((current) => current ? { ...current, opening: false, error: reason instanceof Error ? reason.message : String(reason) } : null);
    }
  };
  const syncCatalog = async () => {
    if (accountStatus.state === 'signed_out' || catalogSyncing) return;
    setCatalogSyncing(true); setError('');
    try { await desktop.syncNow(); await refresh(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setCatalogSyncing(false); void refreshAccountStatus().catch(() => {}); }
  };
  useEffect(() => {
    void refresh().catch((reason) => setError(String(reason)));
    void refreshAccountStatus().catch(() => {});
    if (isDesktop && !updateCheckStarted.current) {
      updateCheckStarted.current = true;
      void checkForUpdates(false).catch(() => {});
    }
  }, []);
  useEffect(() => {
    let disposed = false;
    let unlisten = () => {};
    void desktop.onNativeMenu((action) => {
      setSettingsTab(action === 'about' ? 'about' : 'appearance');
      setModal('settings');
    }).then((dispose) => { if (disposed) dispose(); else unlisten = dispose; }).catch(() => {});
    return () => { disposed = true; unlisten(); };
  }, []);
  useEffect(() => {
    const background = preferences.theme === 'light' ? '#ffffff' : '#0d1012';
    document.documentElement.dataset.theme = preferences.theme;
    document.documentElement.style.backgroundColor = background;
    document.documentElement.style.colorScheme = preferences.theme;
    document.querySelector('meta[name="theme-color"]')?.setAttribute('content', background);
    document.documentElement.lang = preferences.locale === 'zh' ? 'zh-CN' : 'en';
    localStorage.setItem(preferencesStorageKey, JSON.stringify(preferences));
    void desktop.setAppLocale(preferences.locale).catch(() => setError(preferences.locale === 'zh' ? '系统菜单语言切换失败，请重新启动客户端。' : 'The system menu language could not be changed. Restart the app and try again.'));
  }, [preferences]);
  useEffect(() => {
    void desktop.setAppTheme(preferences.theme).catch(() => {});
  }, [preferences.theme]);
  useEffect(() => {
    if (!aiPanelResizing) return;
    const previousCursor = document.body.style.cursor;
    const previousUserSelect = document.body.style.userSelect;
    document.body.style.cursor = 'row-resize';
    document.body.style.userSelect = 'none';
    return () => {
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousUserSelect;
    };
  }, [aiPanelResizing]);
  const adjustAiPanelHeight = (height: number) => {
    setAiPanelHeight(clampAiPanelHeight(height, centerPanel.current?.clientHeight ?? window.innerHeight));
  };
  const startAiPanelDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    aiPanelDrag.current = { pointerId: event.pointerId, startY: event.clientY, startHeight: aiPanelHeight };
    setAiPanelResizing(true);
  };
  const moveAiPanelDrag = (event: PointerEvent<HTMLDivElement>) => {
    const drag = aiPanelDrag.current;
    if (!drag || drag.pointerId !== event.pointerId || !centerPanel.current) return;
    setAiPanelHeight(draggedAiPanelHeight(drag.startHeight, drag.startY, event.clientY, centerPanel.current.clientHeight));
  };
  const finishAiPanelDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (aiPanelDrag.current?.pointerId !== event.pointerId) return;
    aiPanelDrag.current = null;
    setAiPanelResizing(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };
  useEffect(() => {
    if (!folderMenu) return;
    const closeOnOutsideClick = (event: MouseEvent) => {
      if (!(event.target instanceof Element) || !event.target.closest('.folder-actions')) setFolderMenu(null);
    };
    const closeOnEscape = (event: KeyboardEvent) => { if (event.key === 'Escape') setFolderMenu(null); };
    document.addEventListener('mousedown', closeOnOutsideClick);
    document.addEventListener('keydown', closeOnEscape);
    return () => {
      document.removeEventListener('mousedown', closeOnOutsideClick);
      document.removeEventListener('keydown', closeOnEscape);
    };
  }, [folderMenu]);
  useEffect(() => {
    if (!catalogMenu) return;
    const closeOnEscape = (event: KeyboardEvent) => { if (event.key === 'Escape') setCatalogMenu(null); };
    document.addEventListener('keydown', closeOnEscape);
    return () => document.removeEventListener('keydown', closeOnEscape);
  }, [catalogMenu]);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void desktop.onSession((next) => {
      if (next.state === 'disconnected') void desktop.aiDisconnected(next.target).catch(() => {});
      setTabs((current) => current.map((tab) => tab.id === next.target.sessionId ? mergeSession(tab, next) : tab));
    }).then((dispose) => {
      if (disposed) dispose(); else unlisten = dispose;
    });
    return () => { disposed = true; unlisten?.(); };
  }, []);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    void desktop.onSync(() => {
      if (!disposed) setError((current) => current.startsWith('自动同步失败：') || current.startsWith('Automatic sync failed:') ? '' : current);
      void refresh().catch((reason) => setError(String(reason)));
      void refreshAccountStatus().catch(() => {});
    }).then((dispose) => {
      if (disposed) dispose(); else unlisten = dispose;
    });
    void desktop.onSyncError((reason) => {
      if (!disposed) setError(preferences.locale === 'zh' ? `自动同步失败：${reason}` : `Automatic sync failed: ${reason}`);
    }).then((dispose) => {
      if (disposed) dispose(); else unlistenError = dispose;
    });
    return () => { disposed = true; unlisten?.(); unlistenError?.(); };
  }, []);

  const visibleServers = useMemo(() => { const query = search.trim().toLowerCase(); return catalog.servers.filter((server) => `${server.name} ${server.host}`.toLowerCase().includes(query)); }, [catalog.servers, search]);
  const automationTargets = useMemo(() => tabs.flatMap((tab) => {
    const server = catalog.servers.find((item) => item.id === tab.serverId);
    return tab.session?.state === 'connected' && server ? [{ server, session: tab.session }] : [];
  }), [tabs, catalog.servers]);
  const updateTab = (id: string, patch: Partial<TerminalTab>) => setTabs((current) => current.map((tab) => tab.id === id ? { ...tab, ...patch } : tab));
  const connectTab = async (tab: TerminalTab, replace = false) => {
    updateTab(tab.id, { busy: true });
    setError('');
    try {
      const result: ConnectResult = replace ? await desktop.trustHostKey(tab.serverId, tab.id, true) : await desktop.connect(tab.serverId, tab.id);
      setTabs((current) => current.map((item) => {
        if (item.id !== tab.id) return item;
        const merged = result.session ? mergeSession(item, result.session) : { ...item, busy: false };
        return { ...merged, hostKey: result.hostKeyChallenge, busy: false };
      }));
    } catch (reason) {
      updateTab(tab.id, { busy: false });
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  const createTabForServer = (server: ServerRecord) => {
    if (tabs.length >= MAX_TERMINAL_TABS || !isDesktop) return;
    const tab: TerminalTab = { id: crypto.randomUUID(), serverId: server.id, session: null, hostKey: null, busy: true };
    setTabs((current) => [...current, tab]);
    setActiveTabId(tab.id);
    void connectTab(tab);
  };
  const createTab = () => { if (selected) createTabForServer(selected); };
  const openServerConnection = (server: ServerRecord) => {
    setSelectedId(server.id);
    setServerMenu(null);
    const tab = tabs.find((item) => item.serverId === server.id);
    if (!tab) { createTabForServer(server); return; }
    setActiveTabId(tab.id);
    if (!tab.busy && tab.session?.state !== 'connected') void connectTab(tab);
  };
  const closeTab = (id: string) => {
    const tab = tabs.find((item) => item.id === id);
    if (!tab || tab.busy) return;
    const remaining = tabs.filter((item) => item.id !== id);
    setTabs(remaining);
    if (activeTabId === id) setActiveTabId(remaining.at(-1)?.id ?? null);
    void desktop.closeSession(id).catch(() => {});
  };
  const fillCommand = async (command: string) => {
    if (!session || !connected) throw new Error(t('historyNeedsConnection'));
    await desktop.terminalWrite(session.target.sessionId, session.target.generation, Array.from(new TextEncoder().encode(command)));
  };
  const saveFolder = async (event: FormEvent) => {
    event.preventDefault();
    try {
      if (folderEditing) await desktop.saveFolder(folderEditing.id, folderEditing.parentId, folderName);
      else await desktop.createFolder(folderParent, folderName);
      await refresh(); setModal(null); setFolderName(''); setFolderEditing(null);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  };
  const closeTabsForServers = (serverIds: Set<string>) => {
    const removed = tabs.filter((tab) => serverIds.has(tab.serverId));
    if (!removed.length) return;
    const remaining = tabs.filter((tab) => !serverIds.has(tab.serverId));
    setTabs(remaining);
    if (activeTab && serverIds.has(activeTab.serverId)) setActiveTabId(remaining.at(-1)?.id ?? null);
    for (const tab of removed) void desktop.closeSession(tab.id).catch(() => {});
  };
  const removeServer = async (server: ServerRecord, confirmed = false) => {
    setServerMenu(null);
    if (!confirmed) {
      setCatalogDeleteTarget({ kind: 'server', server });
      return;
    }
    setCatalogDeleting(true);
    try {
      await desktop.deleteServer(server.id);
      setCatalogDeleteTarget(null);
      closeTabsForServers(new Set([server.id]));
      await refresh();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setCatalogDeleting(false); }
  };
  const duplicateServer = async (server: ServerRecord) => {
    setServerMenu(null);
    try {
      const duplicate = await desktop.duplicateServer(server.id);
      await refresh(); setSelectedId(duplicate.id);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  };
  const removeFolder = async (folder: FolderRecord, confirmed = false) => {
    setFolderMenu(null);
    const impact = folderDeletionImpact(catalog, folder.id);
    if (impact.serverCount > 0 && !confirmed) {
      setCatalogDeleteTarget({ kind: 'folder', folder, subfolderCount: impact.subfolderCount, serverCount: impact.serverCount });
      return;
    }
    setCatalogDeleting(true);
    try {
      await desktop.deleteFolder(folder.id);
      setCatalogDeleteTarget(null);
      closeTabsForServers(impact.serverIds);
      await refresh();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setCatalogDeleting(false); }
  };
  const toggleServerConnection = async (server: ServerRecord) => {
    setServerMenu(null);
    const tab = tabs.find((item) => item.serverId === server.id);
    if (!tab) {
      createTab();
      return;
    }
    setActiveTabId(tab.id);
    try {
      if (tab.session?.state === 'connected') await desktop.disconnect(tab.id);
      else await connectTab(tab);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  };
  const catalogDragEnabled = !search.trim() && !catalogSaving;
  const startCatalogDrag = (item: CatalogDragItem, event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0 || !catalogDragEnabled || (event.target as HTMLElement).closest('.folder-actions, .server-menu-trigger, .server-context-menu, .context-menu-dismiss')) return;
    draggedCatalogItem.current = item;
    catalogPointerDrag.current = { item, pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, active: false };
    catalogDropDestination.current = null;
    setFolderMenu(null);
    setServerMenu(null);
  };
  const finishCatalogDrag = () => {
    catalogPointerDrag.current = null;
    draggedCatalogItem.current = null;
    catalogDropDestination.current = null;
    document.body.style.userSelect = '';
    setDropIndicator(null);
    setCatalogDragPreview(null);
  };
  const updateCatalogDrag = (event: PointerEvent<HTMLElement>) => {
    const drag = catalogPointerDrag.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (!drag.active && Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < 5) return;
    if (!drag.active) { drag.active = true; event.currentTarget.setPointerCapture(event.pointerId); document.body.style.userSelect = 'none'; }
    event.preventDefault();
    const draggedName = drag.item.kind === 'server'
      ? catalog.servers.find((server) => server.id === drag.item.id)?.name
      : catalog.folders.find((folder) => folder.id === drag.item.id)?.name;
    setCatalogDragPreview({ ...drag.item, name: draggedName ?? '', x: event.clientX, y: event.clientY });
    const hit = document.elementFromPoint(event.clientX, event.clientY);
    const folderElement = hit?.closest<HTMLElement>('[data-catalog-folder-id]');
    let destination: CatalogDropTarget | null = null;
    let indicator: CatalogDropIndicator | null = null;
    if (folderElement) {
      const folderId = folderElement.dataset.catalogFolderId!;
      if (drag.item.kind !== 'folder' || drag.item.id !== folderId) {
        const rect = folderElement.getBoundingClientRect();
        const ratio = (event.clientY - rect.top) / rect.height;
        const position = drag.item.kind === 'server' || ratio >= 0.25 && ratio <= 0.75 ? 'inside' : ratio < 0.25 ? 'before' : 'after';
        const parentId = position === 'inside' ? folderId : folderElement.dataset.catalogParentId || null;
        destination = { parentId, targetId: position === 'inside' ? undefined : folderId, position };
        indicator = { id: folderId, position };
      }
    } else if (hit?.closest('[data-catalog-root]') && !hit.closest('[data-catalog-server-id]')) {
      destination = { parentId: null, position: 'end' };
      indicator = { id: 'root', position: 'end' };
    }
    catalogDropDestination.current = destination;
    setDropIndicator(indicator);
  };
  const endCatalogDrag = (event: PointerEvent<HTMLElement>) => {
    const drag = catalogPointerDrag.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const destination = catalogDropDestination.current;
    if (drag.active) {
      suppressCatalogClick.current = true;
      window.setTimeout(() => { suppressCatalogClick.current = false; }, 0);
      if (destination) { void persistCatalogDrop(destination); return; }
    }
    finishCatalogDrag();
  };
  const indicatorClass = (id: string, position: CatalogDropPosition) => dropIndicator?.id === id && dropIndicator.position === position ? ` catalog-drop-${position}` : '';
  const persistCatalogDrop = async (destination: CatalogDropTarget) => {
    const dragged = draggedCatalogItem.current;
    finishCatalogDrag();
    if (!dragged) return;
    const changes = dragged.kind === 'server'
      ? serverOrderChanges(catalog, dragged.id, destination)
      : folderOrderChanges(catalog, dragged.id, destination);
    if (!changes) {
      setError(preferences.locale === 'zh' ? '不能将文件夹移动到自身或其子文件夹中' : 'A folder cannot be moved into itself or its descendants');
      return;
    }
    if (!changes.length) return;
    setCatalogSaving(true);
    setError('');
    try {
      if (dragged.kind === 'server') await desktop.reorderServers(changes);
      else await desktop.reorderFolders(changes);
      if (destination.parentId) setCollapsed((current) => { const next = new Set(current); next.delete(destination.parentId!); return next; });
      await refresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      await refresh().catch(() => {});
    } finally {
      setCatalogSaving(false);
    }
  };
  const renderServer = (server: ServerRecord, depth: number) => {
    const tab = tabs.find((item) => item.serverId === server.id);
    const serverConnected = tab?.session?.state === 'connected';
    return <div key={server.id} className={`catalog-drag-row${catalogDragPreview?.kind === 'server' && catalogDragPreview.id === server.id ? ' catalog-drag-source' : ''}`} data-catalog-server-id={server.id} data-catalog-draggable={catalogDragEnabled}
      onPointerDown={(event) => startCatalogDrag({ kind: 'server', id: server.id }, event)} onPointerMove={updateCatalogDrag} onPointerUp={endCatalogDrag} onPointerCancel={finishCatalogDrag}
      onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); setSelectedId(server.id); setFolderMenu(null); setServerMenu(server.id); }}>
      <ServerRow server={server} depth={depth} selected={selectedId === server.id} connected={serverConnected} busy={tab?.busy ?? false} menuOpen={serverMenu === server.id} t={t}
        onSelect={() => { setSelectedId(server.id); setServerMenu(null); }}
        onOpen={() => openServerConnection(server)}
        onToggleMenu={() => setServerMenu((current) => current === server.id ? null : server.id)}
        onEdit={() => { setServerMenu(null); setEditing(server); setModal('server'); }}
        onDuplicate={() => void duplicateServer(server)}
        onCopy={() => { setServerMenu(null); void navigator.clipboard.writeText(server.host).catch((reason) => setError(String(reason))); }}
        onConnect={() => void toggleServerConnection(server)}
        onDelete={() => void removeServer(server)} />
    </div>;
  };
  const renderFolder = (folder: FolderRecord, depth: number): ReactNode => {
    const children = catalog.folders.filter((item) => item.parentId === folder.id);
    const servers = visibleServers.filter((server) => server.folderId === folder.id);
    const isCollapsed = collapsed.has(folder.id);
    return <div key={folder.id}><div className={`folder-row${indicatorClass(folder.id, 'before')}${indicatorClass(folder.id, 'after')}${indicatorClass(folder.id, 'inside')}${catalogDragPreview?.kind === 'folder' && catalogDragPreview.id === folder.id ? ' catalog-drag-source' : ''}`} style={{ paddingLeft: 10 + depth * 14 }}
      data-catalog-folder-id={folder.id} data-catalog-parent-id={folder.parentId ?? ''} data-catalog-draggable={catalogDragEnabled}
      onPointerDown={(event) => startCatalogDrag({ kind: 'folder', id: folder.id }, event)} onPointerMove={updateCatalogDrag} onPointerUp={endCatalogDrag} onPointerCancel={finishCatalogDrag}
      onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); setServerMenu(null); setFolderMenu(folder.id); }}>
      <button className="tree-label" onClick={() => setCollapsed((current) => { const next = new Set(current); if (next.has(folder.id)) next.delete(folder.id); else next.add(folder.id); return next; })}>{isCollapsed ? <ChevronRight size={13} /> : <ChevronDown size={13} />}<Folder size={14} /><span>{folder.name}</span></button>
      <div className="folder-actions" onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) setFolderMenu(null); }} onKeyDown={(event) => { if (event.key === 'Escape') setFolderMenu(null); }}>
        <button className="icon small" title={t('folderActions')} aria-label={t('folderActions')} aria-expanded={folderMenu === folder.id} onClick={() => setFolderMenu((current) => current === folder.id ? null : folder.id)}><Plus size={13} /></button>
        {folderMenu === folder.id && <div className="folder-menu" role="menu">
          <button role="menuitem" onClick={() => { setFolderEditing(null); setFolderParent(folder.id); setFolderName(''); setFolderMenu(null); setModal('folder'); }}><FolderPlus size={13} />{t('addSubfolder')}</button>
          <button role="menuitem" onClick={() => { setEditing(null); setFolderParent(folder.id); setFolderMenu(null); setModal('server'); }}><Server size={13} />{t('addServerHere')}</button>
          <button role="menuitem" onClick={() => { setFolderEditing(folder); setFolderParent(folder.parentId); setFolderName(folder.name); setFolderMenu(null); setModal('folder'); }}><Settings2 size={13} />{t('renameFolder')}</button>
          <button role="menuitem" className="danger-menu-item" onClick={() => void removeFolder(folder)}><Trash2 size={13} />{t('deleteFolder')}</button>
        </div>}
      </div>
    </div>{!isCollapsed && <>{children.map((child) => renderFolder(child, depth + 1))}{servers.map((server) => renderServer(server, depth + 1))}</>}</div>;
  };

  return <main className="app-shell" onClickCapture={(event) => { if (suppressCatalogClick.current) { event.preventDefault(); event.stopPropagation(); } }}>
    <header className="titlebar" data-tauri-drag-region="deep"><div className="brand"><span className="brand-mark"><TerminalSquare size={17} /></span><strong>SparrowShell</strong></div><nav><button className="active"><TerminalSquare size={14} />{t('workspace')}</button><button disabled={!automationTargets.length} title={automationTargets.length ? '' : preferences.locale === 'zh' ? '请先连接至少一台服务器' : 'Connect at least one server first'} onClick={() => setModal('automation')}><Play size={14} />{preferences.locale === 'zh' ? '批量与自动化' : 'Automation'}</button></nav><div className="title-actions"><span className="global-status"><span className="status-dot online" />{tabs.filter((tab) => tab.session?.state === 'connected').length}{preferences.locale === 'zh' ? ' 个连接正常' : ' connections online'}</span><button className="language-switch" title={preferences.locale === 'zh' ? 'Switch to English' : '切换为中文'} onClick={() => setPreferences((p) => ({ ...p, locale: p.locale === 'zh' ? 'en' : 'zh' }))}><Globe2 size={14} /><span>{preferences.locale === 'zh' ? '中文' : 'EN'}</span></button><button className="icon notification-button" title={preferences.locale === 'zh' ? '通知' : 'Notifications'} aria-label={preferences.locale === 'zh' ? '通知' : 'Notifications'}><Bell size={16} /><i className="notification-dot" /></button><button className={`icon ${!leftVisible ? 'active' : ''}`} title={leftVisible ? '收起左侧栏' : '展开左侧栏'} onClick={() => setLeftVisible((value) => !value)}><PanelLeftClose size={16} /></button><button className={`icon ${!rightVisible ? 'active' : ''}`} title={rightVisible ? '收起右侧栏' : '展开右侧栏'} onClick={() => setRightVisible((value) => !value)}><PanelRightClose size={16} /></button><button className="icon" onClick={() => setModal('settings')} aria-label={t('settings')}><Settings2 size={16} /></button></div></header>
    {!isDesktop && <div className="preview-banner">{t('preview')}</div>}
    <LocalizedContextMenu className={`workbench ${!leftVisible ? 'left-hidden' : ''} ${!rightVisible ? 'right-hidden' : ''}`} t={t}>
      {leftVisible && <aside className="catalog-panel"><div className="panel-heading"><span>{t('servers')}<b>{catalog.servers.length}</b></span><span className="catalog-actions"><button className="icon" title={accountStatus.state !== 'signed_out' ? t('syncCatalog') : t('syncCatalogLoggedOut')} aria-label={t('syncCatalog')} disabled={!isDesktop || accountStatus.state === 'signed_out' || catalogSyncing} onClick={() => void syncCatalog()}><RefreshCw size={15} className={catalogSyncing ? 'spin' : ''} /></button><button className="icon" title={t('addFolder')} onClick={() => { setFolderEditing(null); setFolderParent(null); setFolderName(''); setModal('folder'); }}><FolderPlus size={16} /></button></span></div>
        <label className="search"><Search size={14} /><input placeholder={t('search')} autoCapitalize="none" autoCorrect="off" spellCheck={false} value={search} onChange={(e) => setSearch(e.target.value)} /></label>
        <button className="new-server" onClick={() => { setEditing(null); setFolderParent(null); setModal('server'); }}><Plus size={15} />{t('addServer')}</button>
        <div className={`tree${indicatorClass('root', 'end')}`} data-catalog-root="true" onContextMenu={(event) => {
          if ((event.target as HTMLElement).closest('[data-catalog-folder-id], [data-catalog-server-id]')) return;
          event.preventDefault();
          event.stopPropagation();
          setFolderMenu(null);
          setServerMenu(null);
          setCatalogMenu({
            x: Math.max(8, Math.min(event.clientX, window.innerWidth - 184)),
            y: Math.max(8, Math.min(event.clientY, window.innerHeight - 100)),
          });
        }}>
          {search.trim() ? <>{visibleServers.map((server) => renderServer(server, 0))}{visibleServers.length === 0 && <div className="catalog-empty"><Search size={26} /><strong>{t('searchNoResults')}</strong></div>}</> : <>{catalog.folders.filter((folder) => !folder.parentId).map((folder) => renderFolder(folder, 0))}{visibleServers.filter((server) => !server.folderId).map((server) => renderServer(server, 0))}</>}
          {catalog.servers.length === 0 && !search.trim() && <div className="catalog-empty"><Server size={26} /><strong>{t('empty')}</strong><p>{t('emptyHelp')}</p></div>}
          {catalogMenu && <><button className="context-menu-dismiss" aria-label={t('cancel')} onClick={() => setCatalogMenu(null)} /><div className="server-context-menu catalog-blank-context-menu" role="menu" aria-label={t('servers')} style={{ left: catalogMenu.x, top: catalogMenu.y }}>
            <button role="menuitem" onClick={() => { setFolderEditing(null); setFolderParent(null); setFolderName(''); setCatalogMenu(null); setModal('folder'); }}><FolderPlus size={14} />{t('addFolder')}</button>
            <button role="menuitem" onClick={() => { setEditing(null); setFolderParent(null); setCatalogMenu(null); setModal('server'); }}><Server size={14} />{t('addServer')}</button>
          </div></>}
        </div>
        <button className="account-entry" onClick={() => setModal('account')}><span className="account-avatar"><ShieldCheck size={16} /></span><span><strong title={accountSession?.email ?? undefined}>{accountSession ? (accountSession.nickname || accountSession.email || accountSession.userId) : (preferences.locale === 'zh' ? '未登录' : 'Signed out')}</strong><small>{accountSession ? (preferences.locale === 'zh' ? '已登录 · 云同步' : 'Signed in · Cloud sync') : (preferences.locale === 'zh' ? '登录后同步配置' : 'Sign in to sync configurations')}</small></span><ChevronRight size={14} /></button>
        <div className="sidebar-note"><ShieldCheck size={16} /><p>{t('secret')}</p></div>
      </aside>}
      <section className="center-panel" ref={centerPanel}>
        {error && <div className="error-banner" role="alert"><span>{error}</span><button className="icon small" onClick={() => setError('')}><X size={14} /></button></div>}
        <div className="terminal-tabs" role="tablist">{tabs.map((tab) => { const server = catalog.servers.find((item) => item.id === tab.serverId); return <div className={`terminal-tab ${tab.id === activeTabId ? 'active' : ''}`} key={tab.id} role="tab" aria-selected={tab.id === activeTabId}><button className="terminal-tab-name" onClick={() => setActiveTabId(tab.id)}><span className={`status-dot ${tab.session?.state === 'connected' ? 'online' : ''}`} /><span>{server?.name ?? t('deletedServer')}</span></button><button className="icon small" title={t('closeTab')} disabled={tab.busy} onClick={() => closeTab(tab.id)}><X size={11} /></button></div>; })}<button className="icon small new-terminal-tab" title={t('newTab')} disabled={!selected || tabs.length >= MAX_TERMINAL_TABS || !isDesktop} onClick={createTab}><Plus size={14} /></button><div className="tab-spacer" />{activeServer && <span className="workspace-path">{connected ? `${t('connected')} · ` : ''}{activeServer.username}@{activeServer.host}:{activeServer.port}</span>}</div>
        <div className="terminal-area">{tabs.map((tab) => tab.session?.channelId ? <Terminal key={tab.id} session={tab.session} fontFamily={preferences.terminalFontFamily} fontSize={preferences.terminalFontSize} completionEnabled={preferences.terminalCompletionEnabled} theme={preferences.theme} locale={preferences.locale} active={tab.id === activeTabId} t={t} onFontFamilyChange={(terminalFontFamily) => setPreferences((p) => ({ ...p, terminalFontFamily }))} onFontSizeChange={(terminalFontSize) => setPreferences((p) => ({ ...p, terminalFontSize }))} onCompletionEnabledChange={(terminalCompletionEnabled) => setPreferences((p) => ({ ...p, terminalCompletionEnabled }))} onHistory={() => setHistoryOpen(true)} onReconnect={() => void connectTab(tab)} onSelection={(selectedOutput) => { if (tab.session) updateAiContext(tab.session.target, { selectedOutput }); }} /> : null)}{!session?.channelId && ((busy || isConnectingState(session?.state)) && activeServer ? <ConnectionFeedback serverName={activeServer.name} state={session?.state} t={t} /> : <div className="welcome"><div className="welcome-icon"><TerminalSquare size={30} /></div><h1>{activeTab ? activeServer?.name : t('welcome')}</h1><p>{t('welcomeDetail')}</p>{activeServer && <button className="button primary" disabled={!isDesktop || busy} onClick={() => activeTab ? void connectTab(activeTab) : createTab()}>{busy ? t('connecting') : activeTab ? t('reconnect') : t('connect')}</button>}</div>)}</div>
        <div
          className={`vertical-resizer${aiPanelResizing ? ' active' : ''}`}
          role="separator"
          aria-label={preferences.locale === 'zh' ? '调整工作台和 AI 运维助手高度' : 'Resize workbench and AI assistant'}
          aria-orientation="horizontal"
          aria-valuemin={MIN_AI_PANEL_HEIGHT}
          aria-valuemax={Math.max(MIN_AI_PANEL_HEIGHT, (centerPanel.current?.clientHeight ?? window.innerHeight) - 260)}
          aria-valuenow={Math.round(aiPanelHeight)}
          title={preferences.locale === 'zh' ? '上下拖动调整区域高度' : 'Drag vertically to resize panels'}
          tabIndex={0}
          onPointerDown={startAiPanelDrag}
          onPointerMove={moveAiPanelDrag}
          onPointerUp={finishAiPanelDrag}
          onPointerCancel={finishAiPanelDrag}
          onKeyDown={(event) => {
            const change = event.key === 'ArrowUp' ? 16 : event.key === 'ArrowDown' ? -16 : 0;
            if (!change) return;
            event.preventDefault();
            adjustAiPanelHeight(aiPanelHeight + change);
          }}
        ><span /></div>
        <div className="ai-panel-slot" style={{ height: aiPanelHeight }}>
          <AiPanel session={session} serverName={activeServer?.name ?? ''} connected={connected} locale={preferences.locale} contextData={session ? aiContexts[`${session.target.serverId}:${session.target.sessionId}:${session.target.generation}`] : undefined} maxSteps={preferences.aiMaxSteps} t={t} />
        </div>
      </section>
      {rightVisible && <aside className="info-panel">{connected && session && activeServer ? <><MonitorPanel session={session} serverName={activeServer.name} host={activeServer.host} username={activeServer.username} locale={preferences.locale} t={t} onSnapshot={(systemInfo) => updateAiContext(session.target, { systemInfo })} /><FilesPanel session={session} serverLabel={`${activeServer.name} · ${activeServer.username}@${activeServer.host}:${activeServer.port}`} theme={preferences.theme} locale={preferences.locale} fontFamily={preferences.terminalFontFamily} fontSize={preferences.terminalFontSize} t={t} initialPath={aiContexts[`${session.target.serverId}:${session.target.sessionId}:${session.target.generation}`]?.workingDirectory ?? null} onDirectory={(workingDirectory, directoryEntries) => updateAiContext(session.target, { workingDirectory, directoryEntries })} /></> : <><div className="panel-heading"><span>{t('local')}</span><ShieldCheck size={16} /></div><div className="info-card"><ShieldCheck size={22} /><h3>{t('noAccount')}</h3><p>{t('secret')}</p></div>{activeServer && <dl className="server-facts"><div><dt>{t('name')}</dt><dd>{activeServer.name}</dd></div><div><dt>{t('host')}</dt><dd>{activeServer.host}</dd></div><div><dt>{t('port')}</dt><dd>{activeServer.port}</dd></div><div><dt>{t('username')}</dt><dd>{activeServer.username}</dd></div><div><dt>{t('authentication')}</dt><dd>{t(activeServer.authType === 'password' ? 'password' : 'privateKey')}</dd></div></dl>}</>}</aside>}
    </LocalizedContextMenu>
    {modal === 'account' && <AccountPanel locale={preferences.locale} initialStatus={accountStatus} onClose={() => { setModal(null); void refreshAccountStatus().catch(() => {}); }} onCatalogChanged={refresh} onAccountChanged={refreshAccountStatus} />}
    {modal === 'automation' && <AutomationPanel targets={automationTargets} locale={preferences.locale} onClose={() => setModal(null)} />}
    {modal === 'server' && <ServerForm server={editing} initialFolderId={folderParent} catalog={catalog} t={t} onClose={() => setModal(null)} onSaved={refresh} />}
    {modal === 'folder' && <Modal title={t(folderEditing ? 'renameFolder' : 'addFolder')} onClose={() => setModal(null)}><form onSubmit={saveFolder} className="simple-form"><label>{t('folderName')}<input required autoFocus maxLength={128} autoCapitalize="none" autoCorrect="off" spellCheck={false} value={folderName} onChange={(e) => setFolderName(e.target.value)} /></label><p className="muted">{folderParent ? catalog.folders.find((folder) => folder.id === folderParent)?.name : t('root')}</p><footer><button type="button" className="button secondary" onClick={() => setModal(null)}>{t('cancel')}</button><button className="button primary">{t(folderEditing ? 'save' : 'create')}</button></footer></form></Modal>}
    {modal === 'settings' && <Modal title={settingsTab === 'appearance' ? t('settings') : preferences.locale === 'zh' ? '关于' : 'About'} onClose={() => setModal(null)}><div className="settings-tabs" role="tablist"><button role="tab" aria-selected={settingsTab === 'appearance'} className={settingsTab === 'appearance' ? 'active' : ''} onClick={() => setSettingsTab('appearance')}>{t('settings')}</button><button role="tab" aria-selected={settingsTab === 'about'} className={settingsTab === 'about' ? 'active' : ''} onClick={() => setSettingsTab('about')}>{preferences.locale === 'zh' ? '关于' : 'About'}</button></div>{settingsTab === 'appearance' ? <div className="settings-form"><label>{t('theme')}<div className="segmented"><button className={preferences.theme === 'dark' ? 'active' : ''} onClick={() => setPreferences((p) => ({ ...p, theme: 'dark' }))}><Moon size={14} />{t('dark')}</button><button className={preferences.theme === 'light' ? 'active' : ''} onClick={() => setPreferences((p) => ({ ...p, theme: 'light' }))}><Sun size={14} />{t('light')}</button></div></label><label>{t('language')}<div className="segmented"><Languages size={15} /><button className={preferences.locale === 'zh' ? 'active' : ''} onClick={() => setPreferences((p) => ({ ...p, locale: 'zh' }))}>简体中文</button><button className={preferences.locale === 'en' ? 'active' : ''} onClick={() => setPreferences((p) => ({ ...p, locale: 'en' }))}>English</button></div></label><label>{t('terminalFont')}<select value={preferences.terminalFontFamily} onChange={(e) => setPreferences((p) => ({ ...p, terminalFontFamily: e.target.value }))}>{terminalFonts.map((font) => <option key={font.label} value={font.value}>{font.label}</option>)}</select></label><label>{t('fontSize')}<TerminalFontSizeInput value={preferences.terminalFontSize} onChange={(terminalFontSize) => setPreferences((p) => ({ ...p, terminalFontSize }))} /></label><label>{t('agentMaxSteps')}<input type="number" min={MIN_DYNAMIC_AGENT_STEPS} max={MAX_DYNAMIC_AGENT_STEPS} step={1} value={preferences.aiMaxSteps} onChange={(event) => { const value = event.currentTarget.valueAsNumber; if (Number.isFinite(value)) setPreferences((p) => ({ ...p, aiMaxSteps: Math.min(MAX_DYNAMIC_AGENT_STEPS, Math.max(MIN_DYNAMIC_AGENT_STEPS, Math.trunc(value))) })); }} /><small className="muted">{t('agentMaxStepsHint')}</small></label></div> : <AboutPanel locale={preferences.locale} onCheckUpdate={() => checkForUpdates(true)} />}</Modal>}
    {updatePrompt && <UpdateDialog update={updatePrompt.update} locale={preferences.locale} manual={updatePrompt.manual} opening={updatePrompt.opening} downloadOpened={updatePrompt.downloadOpened} error={updatePrompt.error} onCancel={() => setUpdatePrompt(null)} onConfirm={() => void confirmUpdate()} />}
    {catalogDeleteTarget && <Modal title={t(catalogDeleteTarget.kind === 'server' ? 'delete' : 'deleteFolder')} onClose={() => { if (!catalogDeleting) setCatalogDeleteTarget(null); }}><div className="simple-form">
      <p className="muted">{t(catalogDeleteTarget.kind === 'server' ? 'confirmDelete' : 'confirmFolderDelete')}</p>
      <strong>{catalogDeleteTarget.kind === 'server' ? catalogDeleteTarget.server.name : catalogDeleteTarget.folder.name}</strong>
      {catalogDeleteTarget.kind === 'folder' && <p className="muted">{t('folderDeleteImpact').replace('{folders}', String(catalogDeleteTarget.subfolderCount)).replace('{servers}', String(catalogDeleteTarget.serverCount))}</p>}
      <footer><button autoFocus className="button secondary" disabled={catalogDeleting} onClick={() => setCatalogDeleteTarget(null)}>{t('cancel')}</button><button className="button danger" disabled={catalogDeleting} onClick={() => void (catalogDeleteTarget.kind === 'server' ? removeServer(catalogDeleteTarget.server, true) : removeFolder(catalogDeleteTarget.folder, true))}><Trash2 size={14} />{catalogDeleting ? t('deleting') : t('confirmDeleteAction')}</button></footer>
    </div></Modal>}
    {hostKey && activeServer && activeTab && <Modal title={hostKey.previousFingerprint ? t('hostKeyChanged') : t('hostKeyNew')} onClose={() => updateTab(activeTab.id, { hostKey: null })}><div className="host-key"><ShieldCheck size={30} /><p>{activeServer.host}:{activeServer.port} · {hostKey.algorithm}</p>{hostKey.previousFingerprint && <p className="error">{hostKey.previousFingerprint}</p>}<label>{t('fingerprint')}<code>{hostKey.fingerprint}</code></label><p>{t('secret')}</p><footer><button className="button secondary" onClick={() => updateTab(activeTab.id, { hostKey: null })}>{t('cancel')}</button><button className="button primary" disabled={busy} onClick={() => void connectTab(activeTab, true)}>{hostKey.previousFingerprint ? t('replace') : t('trust')}</button></footer></div></Modal>}
    {historyOpen && connected && session && <HistoryPanel servers={catalog.servers} currentServerId={session.target.serverId} locale={preferences.locale} t={t} onClose={() => setHistoryOpen(false)} onFill={fillCommand} />}
    {catalogDragPreview && <div className="catalog-drag-preview" role="presentation" style={{ left: catalogDragPreview.x, top: catalogDragPreview.y }}>
      {catalogDragPreview.kind === 'folder' ? <Folder size={14} /> : <Server size={14} />}<span>{catalogDragPreview.name}</span>
    </div>}
  </main>;
}

function ServerRow({ server, depth, selected, connected, busy, menuOpen, t, onSelect, onOpen, onToggleMenu, onEdit, onDuplicate, onCopy, onConnect, onDelete }: { server: ServerRecord; depth: number; selected: boolean; connected: boolean; busy: boolean; menuOpen: boolean; t: (key: MessageKey) => string; onSelect: () => void; onOpen: () => void; onToggleMenu: () => void; onEdit: () => void; onDuplicate: () => void; onCopy: () => void; onConnect: () => void; onDelete: () => void }) {
  return <div className={`server-row ${selected ? 'selected' : ''} ${connected ? 'online' : ''}`} onKeyDown={(event) => { if (event.key === 'Escape' && menuOpen) onToggleMenu(); }}>
    <button className="server-main" style={{ paddingLeft: 12 + depth * 14 }} onClick={onSelect} onDoubleClick={onOpen}><span className="status-dot" /><span><strong>{server.name}</strong><small>{server.username}@{server.host}</small></span></button>
    {selected && <button className="icon small server-menu-trigger" title={t('serverActions')} aria-label={t('serverActions')} aria-expanded={menuOpen} onClick={onToggleMenu}><MoreHorizontal size={15} /></button>}
    {menuOpen && <><button className="context-menu-dismiss" aria-label={t('cancel')} onClick={onToggleMenu} /><div className="server-context-menu" role="menu" aria-label={t('serverActions')}>
      <button role="menuitem" onClick={onEdit}><Settings2 size={14} />{t('edit')}</button>
      <button role="menuitem" onClick={onDuplicate}><Copy size={14} />{t('duplicateServer')}</button>
      <button role="menuitem" onClick={onCopy}><Copy size={14} />{t('copyAddress')}</button>
      <button role="menuitem" disabled={busy || !selected} onClick={onConnect}>{connected ? <XCircle size={14} /> : <Play size={14} />}{busy ? t('connecting') : connected ? t('disconnect') : t('connect')}</button>
      <div className="menu-divider" />
      <button role="menuitem" className="danger-menu-item" onClick={onDelete}><Trash2 size={14} />{t('delete')}</button>
    </div></>}
  </div>;
}
