import { useEffect, useState } from 'react';
import { ArrowDownToLine, History, Search, Trash2, X } from 'lucide-react';
import { desktop } from '../../ipc/client';
import type { Locale, MessageKey } from '../../i18n';
import type { CommandHistoryEntry, ServerRecord } from '../../types';

type Translate = (key: MessageKey) => string;

export function HistoryPanel({ servers, currentServerId, locale, t, onClose, onFill }: {
  servers: ServerRecord[];
  currentServerId: string;
  locale: Locale;
  t: Translate;
  onClose: () => void;
  onFill: (command: string) => Promise<void>;
}) {
  const [entries, setEntries] = useState<CommandHistoryEntry[]>([]);
  const [serverId, setServerId] = useState<string | null>(currentServerId);
  const [search, setSearch] = useState('');
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  const load = async () => {
    setLoading(true);
    setError('');
    try { setEntries(await desktop.commandHistory(serverId, 200)); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setLoading(false); }
  };

  useEffect(() => { void load(); }, [serverId]);

  const clear = async () => {
    if (!window.confirm(t('confirmClearHistory'))) return;
    setBusy(true);
    setError('');
    try {
      await desktop.clearCommandHistory(serverId);
      await load();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(false); }
  };

  const fill = async (command: string) => {
    setBusy(true);
    setError('');
    try { await onFill(command); onClose(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(false); }
  };

  const visible = entries.filter((entry) => entry.command.toLowerCase().includes(search.toLowerCase()));
  const nameOf = (id: string) => servers.find((server) => server.id === id)?.name ?? t('deletedServer');
  return <div className="modal-backdrop"><section className="modal history-modal" role="dialog" aria-modal="true" aria-label={t('commandHistory')}>
    <header><h2><History size={15} />{t('commandHistory')}</h2><button className="icon" onClick={onClose} aria-label={t('cancel')}><X size={17} /></button></header>
    <div className="history-controls"><label className="search"><Search size={14} /><input autoCapitalize="none" autoCorrect="off" spellCheck={false} value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('searchHistory')} /></label><select value={serverId ?? ''} onChange={(event) => setServerId(event.target.value || null)}><option value="">{t('allServers')}</option>{servers.map((server) => <option value={server.id} key={server.id}>{server.name}</option>)}</select><button className="icon danger" title={t('clearHistory')} disabled={busy || loading} onClick={() => void clear()}><Trash2 size={15} /></button></div>
    <p className="history-note">{t('historyCaptureNote')}</p>
    {error && <div className="ops-error" role="alert">{error}</div>}
    <div className="history-list">{loading ? <div className="ops-empty">{t('loadingHistory')}</div> : visible.length === 0 ? <div className="ops-empty">{t('emptyHistory')}</div> : visible.map((entry) => <div className="history-row" key={entry.id}><div><code>{entry.command}</code><small>{nameOf(entry.serverId)} · {new Intl.DateTimeFormat(locale === 'zh' ? 'zh-CN' : 'en-US', { dateStyle: 'short', timeStyle: 'medium' }).format(new Date(entry.createdAt))} · {t('manualTerminal')}</small></div><button className="button secondary" title={t('fillOnly')} disabled={busy} onClick={() => void fill(entry.command)}><ArrowDownToLine size={13} />{t('fillCommand')}</button></div>)}</div>
  </section></div>;
}
