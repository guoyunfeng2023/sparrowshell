import { useCallback, useEffect, useRef, useState } from 'react';
import type { FormEvent, MouseEvent as ReactMouseEvent, ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Archive, ArrowUp, CircleStop, Copy, Download, File, FilePlus, FileText, Folder, FolderInput, FolderOpen, FolderPlus, HardDrive, Link, Pencil, RefreshCw, RotateCcw, Trash2, Upload, X, Zap } from 'lucide-react';
import { desktop, isDesktop } from '../../ipc/client';
import type { MessageKey } from '../../i18n';
import type { RemoteDirectory, RemoteEntry, SessionSnapshot, TransferEvent, TransferState } from '../../types';
import { formatBytes } from './format';
import { RemoteFileEditor } from './RemoteFileEditor';
import { contextMenuPosition, formatRemoteModified, isDragPositionInside, isSupportedArchive, parentRemotePath, remoteArchiveConflictPath, remoteFileError } from './remoteFileUtils';

type Translate = (key: MessageKey) => string;

const stateLabels: Record<TransferState, MessageKey> = {
  queued: 'transferQueued', running: 'transferRunning', completed: 'transferCompleted', failed: 'transferFailed', cancelled: 'transferCancelled',
};

function transferError(event: TransferEvent, t: Translate) {
  if (event.errorCode === 'already_exists') return t('transferAlreadyExists');
  if (event.errorCode === 'invalid_path') return t('transferInvalidPath');
  if (event.errorCode === 'operation_failed') return t('transferOperationFailed');
  return '';
}

function RemoteEntryName({ entry, loading, onOpen }: { entry: RemoteEntry; loading: boolean; onOpen: () => void }) {
  const text = useRef<HTMLSpanElement>(null);
  const [truncated, setTruncated] = useState(false);

  useEffect(() => {
    const element = text.current;
    if (!element) return;
    const measure = () => setTruncated(element.scrollWidth > element.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    window.addEventListener('resize', measure);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [entry.name]);

  return <button className="remote-name" data-tooltip={truncated ? entry.name : undefined} aria-label={entry.name} disabled={(entry.kind !== 'directory' && entry.kind !== 'file') || loading} onDoubleClick={onOpen}>{entry.kind === 'directory' ? <Folder size={14} /> : entry.kind === 'symlink' ? <Link size={14} /> : <File size={14} />}<span ref={text}>{entry.name}</span></button>;
}

type RemoteContextMenuState = { x: number; y: number; entry: RemoteEntry | null };

export function FilesPanel({ session, serverLabel, theme, locale, fontFamily, fontSize, t, initialPath, onDirectory }: { session: SessionSnapshot; serverLabel: string; theme: 'dark' | 'light'; locale: 'zh' | 'en'; fontFamily: string; fontSize: number; t: Translate; initialPath: string | null; onDirectory: (path: string, names: string[]) => void }) {
  const [directory, setDirectory] = useState<RemoteDirectory | null>(null);
  const [transfers, setTransfers] = useState<TransferEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [starting, setStarting] = useState(false);
  const [extracting, setExtracting] = useState(false);
  const [extractConflict, setExtractConflict] = useState<{ entry: RemoteEntry; separateDirectory: boolean; targetPath: string } | null>(null);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [creating, setCreating] = useState<{ kind: 'file' | 'folder'; parent: string } | null>(null);
  const [name, setName] = useState('');
  const [fileAction, setFileAction] = useState<{ type: 'rename' | 'move'; entry: RemoteEntry; value: string } | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<{ entry: RemoteEntry; quick: boolean } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [editingEntry, setEditingEntry] = useState<RemoteEntry | null>(null);
  const [contextMenu, setContextMenu] = useState<RemoteContextMenuState | null>(null);
  const [dropActive, setDropActive] = useState(false);
  const request = useRef(0);
  const currentDirectory = useRef<RemoteDirectory | null>(null);
  const contextMenuRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLElement>(null);

  useEffect(() => { currentDirectory.current = directory; }, [directory]);
  useEffect(() => {
    if ((!deleteTarget && !extractConflict) || deleting) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setDeleteTarget(null);
        setExtractConflict(null);
      }
    };
    document.addEventListener('keydown', closeOnEscape);
    return () => document.removeEventListener('keydown', closeOnEscape);
  }, [deleteTarget, extractConflict, deleting]);
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    const onPointerDown = (event: PointerEvent) => {
      if (!contextMenuRef.current?.contains(event.target as Node)) close();
    };
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === 'Escape') close(); };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('scroll', close, true);
    window.addEventListener('resize', close);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('scroll', close, true);
      window.removeEventListener('resize', close);
    };
  }, [contextMenu]);

  const load = useCallback(async (path: string | null) => {
    if (!isDesktop) return;
    const id = ++request.current;
    setLoading(true);
    setError('');
    try {
      const next = await desktop.listRemoteDirectory(session.target.sessionId, session.target.generation, path);
      if (id === request.current) {
        setDirectory(next);
        onDirectory(next.path, next.entries.map((entry) => entry.name));
      }
    } catch (reason) {
      if (id === request.current) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (id === request.current) setLoading(false);
    }
  }, [session.target.sessionId, session.target.generation]);

  useEffect(() => {
    setDirectory(null);
    setTransfers([]);
    setCreating(null);
    setFileAction(null);
    setDeleteTarget(null);
    setDeleting(false);
    setEditingEntry(null);
    setContextMenu(null);
    setDropActive(false);
    setExtracting(false);
    setExtractConflict(null);
    setError('');
    setNotice('');
    void load(initialPath);
    return () => { request.current += 1; };
  }, [load]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void desktop.onTransfer((event) => {
      if (event.target.sessionId !== session.target.sessionId || event.target.generation !== session.target.generation) return;
      setTransfers((current) => [event, ...current.filter((item) => item.transferId !== event.transferId)].slice(0, 20));
      const directory = currentDirectory.current;
      const parent = event.destination.slice(0, event.destination.lastIndexOf('/')) || '/';
      if (event.state === 'completed' && event.direction === 'upload' && directory?.path === parent) void load(directory.path);
    }).then((dispose) => {
      if (disposed) dispose(); else unlisten = dispose;
    });
    return () => { disposed = true; unlisten?.(); };
  }, [load, session.target.sessionId, session.target.generation]);

  const beginCreate = (kind: 'file' | 'folder', parent: string) => {
    setContextMenu(null);
    setFileAction(null);
    setName('');
    setCreating({ kind, parent });
  };

  const openEntry = (entry: RemoteEntry) => {
    setContextMenu(null);
    setFileAction(null);
    setCreating(null);
    if (entry.kind === 'directory') void load(entry.path);
    if (entry.kind === 'file' && !isSupportedArchive(entry.name)) setEditingEntry(entry);
  };

  const openContextMenu = (event: ReactMouseEvent, entry: RemoteEntry | null) => {
    event.preventDefault();
    event.stopPropagation();
    if (!directory || extracting) return;
    const height = entry === null ? 174 : entry.kind === 'file' ? (isSupportedArchive(entry.name) ? 456 : 416) : entry.kind === 'directory' ? 360 : 210;
    setContextMenu({ entry, ...contextMenuPosition(event.clientX, event.clientY, 196, height, window.innerWidth, window.innerHeight) });
  };

  const startFileAction = (type: 'rename' | 'move', entry: RemoteEntry) => {
    setContextMenu(null);
    setCreating(null);
    setFileAction({ type, entry, value: type === 'rename' ? entry.name : directory?.path ?? '/' });
  };

  const copyFullPath = async (path: string) => {
    setContextMenu(null);
    try { await navigator.clipboard.writeText(path); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  };

  const create = async (event: FormEvent) => {
    event.preventDefault();
    if (!creating || !name.trim()) return;
    const target = creating;
    setLoading(true);
    setError('');
    try {
      if (target.kind === 'file') {
        await desktop.createRemoteFile(session.target.sessionId, session.target.generation, target.parent, name.trim());
      } else {
        await desktop.createRemoteDirectory(session.target.sessionId, session.target.generation, target.parent, name.trim());
      }
      setName('');
      setCreating(null);
      await load(target.parent);
    } catch (reason) {
      setError(remoteFileError(reason, t));
      setLoading(false);
    }
  };

  const applyFileAction = async (event: FormEvent) => {
    event.preventDefault();
    if (!directory || !fileAction || !fileAction.value.trim()) return;
    setLoading(true);
    setError('');
    try {
      if (fileAction.type === 'rename') {
        await desktop.renameRemoteEntry(session.target.sessionId, session.target.generation, fileAction.entry.path, fileAction.value.trim(), fileAction.entry.kind);
      } else {
        await desktop.moveRemoteEntry(session.target.sessionId, session.target.generation, fileAction.entry.path, fileAction.value.trim(), fileAction.entry.kind);
      }
      setFileAction(null);
      await load(directory.path);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setLoading(false);
    }
  };

  const remove = async (target: { entry: RemoteEntry; quick: boolean }) => {
    if (!directory || deleting) return;
    setDeleting(true);
    setError('');
    try {
      const { entry } = target;
      if (target.quick) {
        await desktop.quickRemoveRemoteEntry(session.target.sessionId, session.target.generation, entry.path, entry.kind);
      } else {
        await desktop.removeRemoteEntry(session.target.sessionId, session.target.generation, entry.path, entry.kind);
      }
      setDeleteTarget(null);
      await load(directory.path);
    } catch (reason) {
      setDeleteTarget(null);
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally { setDeleting(false); }
  };

  const uploadPaths = useCallback(async (localPaths: string[]) => {
    if (!directory || starting || localPaths.length === 0) return;
    const targetDirectory = directory.path;
    const sessionId = session.target.sessionId;
    const generation = session.target.generation;
    setStarting(true);
    setError('');
    let firstError = '';
    for (const localPath of localPaths) {
      const name = localPath.split('/').filter(Boolean).pop();
      if (!name) {
        firstError ||= t('transferInvalidPath');
        continue;
      }
      const remotePath = `${targetDirectory === '/' ? '' : targetDirectory}/${name}`;
      try {
        await desktop.startUpload(sessionId, generation, localPath, remotePath, false);
      } catch (reason) {
        firstError ||= reason instanceof Error ? reason.message : String(reason);
      }
    }
    if (firstError) setError(firstError);
    setStarting(false);
  }, [directory, starting, session.target.sessionId, session.target.generation, t]);

  const upload = async () => {
    if (!directory || starting) return;
    setStarting(true);
    setError('');
    try {
      const localPath = await desktop.selectLocalFile();
      if (localPath) await uploadPaths([localPath]);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally { setStarting(false); }
  };

  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const canDrop = (position: { x: number; y: number }) => {
      const rect = panelRef.current?.getBoundingClientRect();
      return Boolean(directory && !starting && rect && isDragPositionInside(position, rect));
    };
    // The main window emits Finder drops to Window/WebviewWindow targets.
    // A Webview-target listener never receives these events on macOS.
    void getCurrentWindow().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === 'enter' || payload.type === 'over') {
        setDropActive(canDrop(payload.position));
      } else if (payload.type === 'drop') {
        const accepted = canDrop(payload.position);
        setDropActive(false);
        if (accepted) void uploadPaths(payload.paths);
      } else {
        setDropActive(false);
      }
    }).then((dispose) => {
      if (disposed) dispose(); else unlisten = dispose;
    }).catch(() => setDropActive(false));
    return () => {
      disposed = true;
      unlisten?.();
      setDropActive(false);
    };
  }, [directory, starting, uploadPaths]);

  const extract = async (entry: RemoteEntry, separateDirectory: boolean, overwrite = false) => {
    if (!directory || entry.kind !== 'file' || extracting || !isSupportedArchive(entry.name)) return;
    setContextMenu(null);
    setExtractConflict(null);
    setExtracting(true);
    setError('');
    setNotice('');
    try {
      const destination = await desktop.extractRemoteArchive(session.target.sessionId, session.target.generation, entry.path, separateDirectory, overwrite);
      await load(directory.path);
      setNotice(`${t('archiveExtracted')} — ${destination}`);
    } catch (reason) {
      const targetPath = remoteArchiveConflictPath(reason);
      if (!overwrite && targetPath !== null) {
        setExtractConflict({ entry, separateDirectory, targetPath: targetPath || entry.path });
      } else {
        setError(remoteFileError(reason, t));
      }
    } finally { setExtracting(false); }
  };

  const download = async (entry: RemoteEntry) => {
    if (entry.kind !== 'file' || starting) return;
    setStarting(true);
    setError('');
    try {
      const localPath = await desktop.selectLocalDestination(entry.name);
      if (!localPath) return;
      await desktop.startDownload(session.target.sessionId, session.target.generation, entry.path, localPath, false);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally { setStarting(false); }
  };

  const retryOverwrite = async (transfer: TransferEvent) => {
    if (starting || !window.confirm(`${t('confirmOverwrite')}\n\n${transfer.destination}`)) return;
    setStarting(true);
    setError('');
    try {
      if (transfer.direction === 'upload') {
        await desktop.startUpload(session.target.sessionId, session.target.generation, transfer.source, transfer.destination, true);
      } else {
        await desktop.startDownload(session.target.sessionId, session.target.generation, transfer.source, transfer.destination, true);
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally { setStarting(false); }
  };

  const cancelTransfer = async (transferId: string) => {
    try {
      await desktop.cancelTransfer(transferId, session.target.sessionId, session.target.generation);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  };

  const menuItem = (label: string, icon: ReactNode, action: () => void, danger = false) => <button role="menuitem" className={danger ? 'danger-menu-item' : ''} onClick={action}>{icon}<span>{label}</span></button>;
  const hasCompletedTransfers = transfers.some((transfer) => transfer.state === 'completed');
  const parent = directory && directory.path !== '/' ? directory.path.slice(0, directory.path.lastIndexOf('/')) || '/' : null;
  return <section ref={panelRef} className={`files-panel${dropActive ? ' drop-active' : ''}`}>
    <div className="panel-heading"><span><HardDrive size={14} />{t('files')}</span><span className="file-actions"><button className="icon small" title={t('upload')} disabled={!directory || starting} onClick={() => void upload()}><Upload size={13} /></button><button className="icon small" title={t('newRemoteFile')} disabled={!directory || loading} onClick={() => directory && beginCreate('file', directory.path)}><FilePlus size={13} /></button><button className="icon small" title={t('newRemoteFolder')} disabled={!directory || loading} onClick={() => directory && beginCreate('folder', directory.path)}><FolderPlus size={13} /></button><button className="icon small" title={t('refresh')} disabled={loading || !isDesktop} onClick={() => void load(directory?.path ?? null)}><RefreshCw size={13} className={loading ? 'spin' : ''} /></button></span></div>
    {dropActive && directory && <div className="remote-drop-overlay" role="status"><Upload size={24} /><strong>{t('dropFilesToUpload')}</strong><code>{directory.path}</code><span>{t('dropFilesOnly')}</span></div>}
    {!isDesktop ? <div className="ops-empty">{t('filesPreview')}</div> : <>
      <div className="path-bar"><button className="icon small" disabled={!parent || loading} title={t('parentDirectory')} onClick={() => { setFileAction(null); void load(parent); }}><ArrowUp size={13} /></button><code title={directory?.path}>{directory?.path ?? '—'}</code></div>
      {creating && <form className="remote-folder-form remote-entry-form" onSubmit={create}><input autoFocus maxLength={255} required autoCapitalize="none" autoCorrect="off" spellCheck={false} value={name} onChange={(event) => setName(event.target.value)} placeholder={t(creating.kind === 'file' ? 'fileName' : 'folderName')} /><button className="button primary" disabled={loading}>{t('create')}</button><button type="button" className="icon small" title={t('cancel')} disabled={loading} onClick={() => setCreating(null)}><X size={12} /></button><small title={creating.parent}>{creating.parent}</small></form>}
      {fileAction && <form className="remote-folder-form remote-entry-form" onSubmit={applyFileAction}><input autoFocus aria-label={t(fileAction.type === 'rename' ? 'newName' : 'destinationDirectory')} maxLength={fileAction.type === 'rename' ? 255 : 4096} required autoCapitalize="none" autoCorrect="off" spellCheck={false} value={fileAction.value} onChange={(event) => setFileAction((current) => current ? { ...current, value: event.target.value } : null)} placeholder={t(fileAction.type === 'rename' ? 'newName' : 'destinationDirectory')} /><button className="button primary" disabled={loading}>{t(fileAction.type === 'rename' ? 'renameRemote' : 'moveRemote')}</button><button type="button" className="icon small" title={t('cancel')} disabled={loading} onClick={() => setFileAction(null)}><X size={12} /></button><small>{t('remoteActionHint')}</small></form>}
      {error && <div className="ops-error" role="alert">{error}</div>}
      {directory?.truncated && <div className="stale-banner">{t('directoryTruncated')}</div>}
      <div className="remote-list" onContextMenu={(event) => openContextMenu(event, null)}>{!directory && <div className="ops-empty">{loading ? t('loadingDirectory') : t('noDirectory')}</div>}{directory?.entries.length === 0 && <div className="ops-empty">{t('emptyDirectory')}</div>}{directory?.entries.map((entry) => <div className={`remote-row ${entry.kind === 'directory' ? 'is-directory' : ''}`} key={entry.path} onContextMenu={(event) => openContextMenu(event, entry)}>
        <RemoteEntryName entry={entry} loading={loading || extracting} onOpen={() => openEntry(entry)} />
        <small>{entry.kind === 'file' && entry.size !== null ? formatBytes(entry.size) : t(entry.kind === 'directory' ? 'directory' : entry.kind === 'symlink' ? 'symlink' : entry.kind === 'file' ? 'file' : 'other')}</small>
        <span className="remote-row-tail"><small className="remote-modified">{formatRemoteModified(entry.modifiedAt)}</small><span className="remote-actions">{entry.kind === 'file' && <button className="icon small" title={t('download')} disabled={starting || loading || extracting} onClick={() => void download(entry)}><Download size={12} /></button>}{entry.kind !== 'other' && <><button className="icon small" title={t('renameRemote')} disabled={loading || extracting} onClick={() => startFileAction('rename', entry)}><Pencil size={12} /></button><button className="icon small" title={t('moveRemote')} disabled={loading || extracting} onClick={() => startFileAction('move', entry)}><FolderInput size={12} /></button><button className="icon small danger" title={t(entry.kind === 'directory' ? 'deleteFolder' : 'deleteFile')} aria-label={t(entry.kind === 'directory' ? 'deleteFolder' : 'deleteFile')} disabled={loading || deleting || extracting} onClick={() => setDeleteTarget({ entry, quick: false })}><Trash2 size={12} /></button><button className="icon small danger" title={t('quickDelete')} aria-label={t('quickDelete')} disabled={loading || deleting || extracting} onClick={() => setDeleteTarget({ entry, quick: true })}><Zap size={12} /></button></>}</span></span>
      </div>)}</div>
      {extracting && <div className="stale-banner remote-extraction-status" role="status"><RefreshCw size={11} className="spin" /> {t('extractingArchive')}</div>}
      {notice && !extracting && <div className="remote-file-notice remote-extraction-status" role="status"><Archive size={12} /><span>{notice}</span><button className="icon small" title={t('dismissNotice')} aria-label={t('dismissNotice')} onClick={() => setNotice('')}><X size={11} /></button></div>}
      {transfers.length > 0 && <div className="transfer-list"><div className="transfer-heading">{t('transfers')}<span>{transfers.filter((item) => item.state === 'queued' || item.state === 'running').length}</span>{hasCompletedTransfers && <button className="transfer-clear" onClick={() => setTransfers((current) => current.filter((item) => item.state !== 'completed'))}>{t('clearCompletedTransfers')}</button>}</div>{transfers.map((transfer) => {
        const active = transfer.state === 'queued' || transfer.state === 'running';
        const message = transferError(transfer, t);
        return <div className={`transfer-row ${transfer.state}`} key={transfer.transferId} title={`${transfer.source} → ${transfer.destination}`}>
          <div className="transfer-title">{transfer.direction === 'upload' ? <Upload size={11} /> : <Download size={11} />}<span>{transfer.destination.split('/').pop()}</span><small>{t(stateLabels[transfer.state])}</small>{active && <button className="icon small" title={t('cancelTransfer')} onClick={() => void cancelTransfer(transfer.transferId)}><CircleStop size={12} /></button>}{transfer.state === 'failed' && transfer.errorCode === 'already_exists' && <button className="icon small" title={t('retryOverwrite')} disabled={starting} onClick={() => void retryOverwrite(transfer)}><RotateCcw size={12} /></button>}{!active && <button className="icon small transfer-dismiss" title={t('dismissTransfer')} aria-label={t('dismissTransfer')} onClick={() => setTransfers((current) => current.filter((item) => item.transferId !== transfer.transferId))}><X size={12} /></button>}</div>
          <progress max={transfer.bytesTotal || 100} value={transfer.bytesTotal ? Math.min(transfer.bytesDone, transfer.bytesTotal) : transfer.state === 'completed' ? 100 : 0} />
          <div className="transfer-meta"><span>{formatBytes(transfer.bytesDone)}{transfer.bytesTotal > 0 ? ` / ${formatBytes(transfer.bytesTotal)}` : ''}</span><span>{active && transfer.bytesPerSecond > 0 ? `${formatBytes(transfer.bytesPerSecond)}/s` : message}</span></div>
        </div>;
      })}</div>}
    </>}
    {deleteTarget && <div className="modal-backdrop"><div className="modal remote-delete-modal" role="alertdialog" aria-modal="true" aria-labelledby="remote-delete-title" aria-describedby="remote-delete-description"><header><h2 id="remote-delete-title">{t(deleteTarget.quick ? 'quickDelete' : deleteTarget.entry.kind === 'directory' ? 'deleteFolder' : 'deleteFile')}</h2><button className="icon" title={t('cancel')} aria-label={t('cancel')} disabled={deleting} onClick={() => setDeleteTarget(null)}><X size={15} /></button></header><div className="remote-delete-content"><p id="remote-delete-description">{t(deleteTarget.quick ? 'confirmQuickFileDelete' : 'confirmFileDelete')}</p><strong>{deleteTarget.entry.name}</strong><code>{serverLabel}</code><code>{deleteTarget.entry.path}</code><footer><button autoFocus className="button secondary" disabled={deleting} onClick={() => setDeleteTarget(null)}>{t('cancel')}</button><button className="button danger" disabled={deleting} onClick={() => void remove(deleteTarget)}>{deleteTarget.quick ? <Zap size={13} /> : <Trash2 size={13} />}{deleting ? t('deleting') : t(deleteTarget.quick ? 'confirmQuickDeleteAction' : 'confirmDeleteAction')}</button></footer></div></div></div>}
    {extractConflict && <div className="modal-backdrop"><div className="modal remote-delete-modal" role="alertdialog" aria-modal="true" aria-labelledby="archive-overwrite-title" aria-describedby="archive-overwrite-description"><header><h2 id="archive-overwrite-title">{t(extractConflict.targetPath.endsWith('/') ? 'archiveConflictFolder' : 'archiveOverwriteTitle')}</h2><button className="icon" title={t('cancel')} aria-label={t('cancel')} disabled={extracting} onClick={() => setExtractConflict(null)}><X size={15} /></button></header><div className="remote-delete-content"><p id="archive-overwrite-description">{t(extractConflict.targetPath.endsWith('/') ? 'confirmArchiveOverwriteFolder' : 'confirmArchiveOverwrite')}</p><strong>{t(extractConflict.targetPath.endsWith('/') ? 'archiveConflictFolder' : 'archiveConflictTarget')}：{extractConflict.targetPath.split('/').filter(Boolean).pop() || extractConflict.targetPath}</strong><code>{serverLabel}</code><code>{extractConflict.targetPath}</code><footer><button autoFocus className="button secondary" disabled={extracting} onClick={() => setExtractConflict(null)}>{t('cancel')}</button><button className="button danger" disabled={extracting} onClick={() => void extract(extractConflict.entry, extractConflict.separateDirectory, true)}><Archive size={13} />{t('overwriteAndExtract')}</button></footer></div></div></div>}
    {editingEntry && <RemoteFileEditor key={`${session.target.sessionId}:${editingEntry.path}`} sessionId={session.target.sessionId} generation={session.target.generation} path={editingEntry.path} theme={theme} locale={locale} fontFamily={fontFamily} fontSize={fontSize} t={t} onClose={() => setEditingEntry(null)} onSaved={() => { if (directory) void load(directory.path); }} />}
    {contextMenu && createPortal(<div ref={contextMenuRef} className="remote-context-menu" role="menu" aria-label={t('files')} style={{ left: contextMenu.x, top: contextMenu.y }} onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); }}>
      {contextMenu.entry === null && directory && <>
        {menuItem(t('newRemoteFile'), <FilePlus size={13} />, () => beginCreate('file', directory.path))}
        {menuItem(t('newRemoteFolder'), <FolderPlus size={13} />, () => beginCreate('folder', directory.path))}
        <div className="menu-divider" />
        {menuItem(t('upload'), <Upload size={13} />, () => { setContextMenu(null); void upload(); })}
        {menuItem(t('refresh'), <RefreshCw size={13} />, () => { setContextMenu(null); void load(directory.path); })}
      </>}
      {contextMenu.entry?.kind === 'file' && (() => { const entry = contextMenu.entry as RemoteEntry; return <>
        {!isSupportedArchive(entry.name) && menuItem(t('openRemoteFile'), <FileText size={13} />, () => openEntry(entry))}
        {menuItem(t('openContainingFolder'), <FolderOpen size={13} />, () => { setContextMenu(null); void load(parentRemotePath(entry.path)); })}
        <div className="menu-divider" />
        {menuItem(t('newRemoteFile'), <FilePlus size={13} />, () => beginCreate('file', parentRemotePath(entry.path)))}
        {menuItem(t('newRemoteFolder'), <FolderPlus size={13} />, () => beginCreate('folder', parentRemotePath(entry.path)))}
        {menuItem(t('download'), <Download size={13} />, () => { setContextMenu(null); void download(entry); })}
        {isSupportedArchive(entry.name) && <>
          {menuItem(t('extractArchiveSeparate'), <Archive size={13} />, () => void extract(entry, true))}
          {menuItem(t('extractArchiveHere'), <FolderOpen size={13} />, () => void extract(entry, false))}
        </>}
        <div className="menu-divider" />
        {menuItem(t('renameRemote'), <Pencil size={13} />, () => startFileAction('rename', entry))}
        {menuItem(t('moveRemote'), <FolderInput size={13} />, () => startFileAction('move', entry))}
        {menuItem(t('copyFullPath'), <Copy size={13} />, () => void copyFullPath(entry.path))}
        <div className="menu-divider" />
        {menuItem(t('deleteFile'), <Trash2 size={13} />, () => { setContextMenu(null); setDeleteTarget({ entry, quick: false }); }, true)}
        {menuItem(t('quickDelete'), <Zap size={13} />, () => { setContextMenu(null); setDeleteTarget({ entry, quick: true }); }, true)}
      </>; })()}
      {contextMenu.entry?.kind === 'directory' && (() => { const entry = contextMenu.entry as RemoteEntry; return <>
        {menuItem(t('openFolder'), <FolderOpen size={13} />, () => openEntry(entry))}
        {menuItem(t('newRemoteFile'), <FilePlus size={13} />, () => beginCreate('file', entry.path))}
        {menuItem(t('newRemoteFolder'), <FolderPlus size={13} />, () => beginCreate('folder', entry.path))}
        <div className="menu-divider" />
        {menuItem(t('renameRemote'), <Pencil size={13} />, () => startFileAction('rename', entry))}
        {menuItem(t('moveRemote'), <FolderInput size={13} />, () => startFileAction('move', entry))}
        {menuItem(t('copyFullPath'), <Copy size={13} />, () => void copyFullPath(entry.path))}
        <div className="menu-divider" />
        {menuItem(t('deleteFolder'), <Trash2 size={13} />, () => { setContextMenu(null); setDeleteTarget({ entry, quick: false }); }, true)}
        {menuItem(t('quickDelete'), <Zap size={13} />, () => { setContextMenu(null); setDeleteTarget({ entry, quick: true }); }, true)}
      </>; })()}
      {contextMenu.entry?.kind === 'symlink' && (() => { const entry = contextMenu.entry as RemoteEntry; return <>
        {menuItem(t('renameRemote'), <Pencil size={13} />, () => startFileAction('rename', entry))}
        {menuItem(t('moveRemote'), <FolderInput size={13} />, () => startFileAction('move', entry))}
        {menuItem(t('copyFullPath'), <Copy size={13} />, () => void copyFullPath(entry.path))}
        {menuItem(t('deleteFile'), <Trash2 size={13} />, () => { setContextMenu(null); setDeleteTarget({ entry, quick: false }); }, true)}
        {menuItem(t('quickDelete'), <Zap size={13} />, () => { setContextMenu(null); setDeleteTarget({ entry, quick: true }); }, true)}
      </>; })()}
      {contextMenu.entry?.kind === 'other' && menuItem(t('copyFullPath'), <Copy size={13} />, () => void copyFullPath((contextMenu.entry as RemoteEntry).path))}
    </div>, document.body)}
  </section>;
}
