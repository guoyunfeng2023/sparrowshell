import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import type { ReactCodeMirrorRef } from '@uiw/react-codemirror';
import { oneDark } from '@codemirror/theme-one-dark';
import { closeSearchPanel, openSearchPanel } from '@codemirror/search';
import type { LanguageSupport } from '@codemirror/language';
import { FileText, RefreshCw, Save, Search, X } from 'lucide-react';
import { desktop } from '../../ipc/client';
import type { Locale, MessageKey } from '../../i18n';
import { remoteEditorLanguageDescription } from './remoteEditorLanguage';
import { remoteEditorSearchExtensions } from './remoteEditorSearch';
import { remoteEditorLightHighlighting } from './remoteEditorTheme';
import { remoteFileError } from './remoteFileUtils';

type Props = {
  sessionId: string;
  generation: number;
  path: string;
  theme: 'dark' | 'light';
  locale: Locale;
  fontFamily: string;
  fontSize: number;
  t: (key: MessageKey) => string;
  onClose: () => void;
  onSaved: () => void;
};

export function RemoteFileEditor({ sessionId, generation, path, theme, locale, fontFamily, fontSize, t, onClose, onSaved }: Props) {
  const [content, setContent] = useState('');
  const [savedContent, setSavedContent] = useState('');
  const [loaded, setLoaded] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [savedNotice, setSavedNotice] = useState(false);
  const [error, setError] = useState('');
  const [language, setLanguage] = useState<LanguageSupport | null>(null);
  const request = useRef(0);
  const editor = useRef<ReactCodeMirrorRef>(null);
  const translate = useRef(t);
  translate.current = t;
  const dirty = content !== savedContent;
  const name = path.split('/').pop() || path;
  const extensions = useMemo(() => [
    ...remoteEditorSearchExtensions(locale),
    ...(theme === 'light' ? [remoteEditorLightHighlighting] : []),
    ...(language ? [language] : []),
  ], [language, locale, theme]);
  const editorStyle = {
    '--remote-editor-font-family': fontFamily,
    '--remote-editor-font-size': `${fontSize}px`,
  } as CSSProperties;

  useEffect(() => {
    let cancelled = false;
    setLanguage(null);
    const description = remoteEditorLanguageDescription(path);
    if (!description) return () => { cancelled = true; };
    void description.load().then((support) => {
      if (!cancelled) setLanguage(support);
    }).catch(() => {
      // An unavailable parser should never prevent editing the remote text.
    });
    return () => { cancelled = true; };
  }, [path]);

  const readFile = useCallback(async () => {
    const id = ++request.current;
    setLoading(true);
    setError('');
    setSavedNotice(false);
    try {
      const next = await desktop.readRemoteTextFile(sessionId, generation, path);
      if (id !== request.current) return;
      setContent(next);
      setSavedContent(next);
      setLoaded(true);
    } catch (reason) {
      if (id === request.current) setError(remoteFileError(reason, translate.current));
    } finally {
      if (id === request.current) setLoading(false);
    }
  }, [sessionId, generation, path]);

  useEffect(() => {
    void readFile();
    return () => { request.current += 1; };
  }, [readFile]);

  const requestClose = useCallback(() => {
    if (saving) return;
    if (dirty && !window.confirm(t('confirmDiscardRemoteChanges'))) return;
    onClose();
  }, [dirty, saving, t, onClose]);

  const reload = useCallback(() => {
    if (loading || saving) return;
    if (dirty && !window.confirm(t('confirmDiscardRemoteChanges'))) return;
    void readFile();
  }, [dirty, loading, saving, readFile, t]);

  const openSearch = useCallback(() => {
    const view = editor.current?.view;
    if (view) openSearchPanel(view);
  }, []);

  const save = useCallback(async () => {
    if (loading || saving || !loaded) return;
    const nextContent = content;
    setSaving(true);
    setError('');
    setSavedNotice(false);
    try {
      await desktop.writeRemoteTextFile(sessionId, generation, path, nextContent);
      setSavedContent(nextContent);
      setSavedNotice(true);
      onSaved();
    } catch (reason) {
      setError(remoteFileError(reason, t));
    } finally {
      setSaving(false);
    }
  }, [content, loading, saving, loaded, sessionId, generation, path, onSaved, t]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
        event.preventDefault();
        void save();
      } else if (event.key === 'Escape') {
        const view = editor.current?.view;
        event.preventDefault();
        if (view && closeSearchPanel(view)) return;
        requestClose();
      }
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [save, requestClose]);

  return <div className="modal-backdrop remote-editor-backdrop">
    <div className="modal remote-editor-modal" role="dialog" aria-modal="true" aria-labelledby="remote-editor-title">
      <header>
        <div className="remote-editor-title"><FileText size={17} /><div><h2 id="remote-editor-title">{name}</h2><span>{dirty ? t('remoteFileModified') : t('remoteFileSaved')}</span></div></div>
        <button className="icon" title={t('close')} aria-label={t('close')} disabled={saving} onClick={requestClose}><X size={16} /></button>
      </header>
      <div className="remote-editor-path"><code title={path}>{path}</code></div>
      <div className="remote-editor-toolbar">
        <span>{t('remoteEditorHint')}</span>
        <div><button className="button secondary small" disabled={loading || saving || !loaded} onClick={openSearch}><Search size={12} />{t('remoteEditorSearchReplace')}</button><button className="button secondary small" disabled={loading || saving} onClick={reload}><RefreshCw size={12} className={loading ? 'spin' : ''} />{t('reloadRemoteFile')}</button><button className="button primary small" disabled={loading || saving || !loaded || !dirty} onClick={() => void save()}><Save size={12} />{saving ? t('savingRemoteFile') : t('save')}</button></div>
      </div>
      {error && <div className="ops-error remote-editor-error" role="alert">{error}</div>}
      {savedNotice && !dirty && <div className="remote-editor-success" role="status">{t('remoteFileSaveSuccess')}</div>}
      <div className="remote-editor-body" style={editorStyle}>
        {!loaded && loading ? <div className="ops-empty">{t('loadingRemoteFile')}</div> : <CodeMirror
          ref={editor}
          className="remote-editor-code"
          value={content}
          height="100%"
          theme={theme === 'dark' ? oneDark : 'light'}
          extensions={extensions}
          autoFocus
          readOnly={loading}
          editable={!loading}
          onCreateEditor={(view) => { view.contentDOM.setAttribute('aria-label', t('remoteFileContent')); }}
          basicSetup={{ autocompletion: false }}
          onChange={(value) => { setContent(value); setSavedNotice(false); }}
        />}
      </div>
      <footer className="remote-editor-footer"><span>{t('remoteFileLocalOnly')}</span><span>{t('remoteEditorSearchShortcut')} · {t('remoteSaveShortcut')}</span></footer>
    </div>
  </div>;
}
