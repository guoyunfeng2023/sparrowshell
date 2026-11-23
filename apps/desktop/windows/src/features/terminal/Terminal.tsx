import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Terminal as Xterm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { Clock3, RotateCcw, Sparkles, Square, Trash2, X } from 'lucide-react';
import { desktop } from '../../ipc/client';
import type { Locale, MessageKey } from '../../i18n';
import type { CommandHistoryEntry, SessionSnapshot } from '../../types';
import { CommandLineRecorder } from './commandRecorder';
import { getCommandCompletions, type CommandCompletion } from './commandCompletion';
import { getCompletionPosition, type CompletionPosition } from './completionPosition';
import { shouldDismissCompletion } from './completionDismissal';
import { TerminalFontSizeInput, terminalFonts } from './terminalOptions';
import { getCompletionSelectionIndex, shouldBypassTerminalKeyEvent, shouldSelectCompletionOnPointerMove, terminalCompletionKeyAction, type CompletionPointerPosition } from './terminalKeyboard';

type Translate = (key: MessageKey) => string;

type CompletionState = {
  items: CommandCompletion[];
  selected: number;
  input: string;
  selectionActive: boolean;
};

const emptyCompletion: CompletionState = { items: [], selected: 0, input: '', selectionActive: false };

const terminalThemes = {
  dark: { background: '#0b0f10', foreground: '#d7dedd', cursor: '#65d6b1', selectionBackground: '#26453c' },
  light: { background: '#f7f9f8', foreground: '#26332f', cursor: '#147a5b', selectionBackground: '#d5e9df' },
} as const;

export function Terminal({ session, fontFamily, fontSize, completionEnabled, theme, locale, active, t, onFontFamilyChange, onFontSizeChange, onCompletionEnabledChange, onHistory, onReconnect, onSelection }: { session: SessionSnapshot; fontFamily: string; fontSize: number; completionEnabled: boolean; theme: 'dark' | 'light'; locale: Locale; active: boolean; t: Translate; onFontFamilyChange: (fontFamily: string) => void; onFontSizeChange: (fontSize: number) => void; onCompletionEnabledChange: (enabled: boolean) => void; onHistory: () => void; onReconnect: () => void; onSelection: (selection: string) => void }) {
  const instance = useRef<HTMLElement>(null);
  const host = useRef<HTMLDivElement>(null);
  const completionPanel = useRef<HTMLElement>(null);
  const completionList = useRef<HTMLDivElement>(null);
  const terminal = useRef<Xterm | null>(null);
  const fitAddon = useRef<FitAddon | null>(null);
  const recorder = useRef<CommandLineRecorder | null>(null);
  const history = useRef<CommandHistoryEntry[]>([]);
  const current = useRef(session);
  const completion = useRef<CompletionState>(emptyCompletion);
  const completionPointer = useRef<CompletionPointerPosition | null>(null);
  const completionEnabledCurrent = useRef(completionEnabled);
  completionEnabledCurrent.current = completionEnabled;
  const [completionState, setCompletionState] = useState<CompletionState>(emptyCompletion);
  const [completionPosition, setCompletionPosition] = useState<CompletionPosition>({ left: 12, top: 41, maxHeight: 320 });

  const positionCompletion = () => {
    const container = instance.current;
    const xterm = terminal.current;
    const screen = host.current?.querySelector<HTMLElement>('.xterm-screen');
    if (!container || !xterm || !screen || xterm.cols === 0 || xterm.rows === 0) return;
    const containerRect = container.getBoundingClientRect();
    const screenRect = screen.getBoundingClientRect();
    if (containerRect.width === 0 || containerRect.height === 0 || screenRect.width === 0 || screenRect.height === 0) return;
    const cellWidth = screenRect.width / xterm.cols;
    const cellHeight = screenRect.height / xterm.rows;
    const next = getCompletionPosition({
      containerWidth: containerRect.width,
      containerHeight: containerRect.height,
      cursorLeft: screenRect.left - containerRect.left + xterm.buffer.active.cursorX * cellWidth,
      cursorTop: screenRect.top - containerRect.top + xterm.buffer.active.cursorY * cellHeight,
      cursorWidth: cellWidth,
      cursorHeight: cellHeight,
      popupWidth: completionPanel.current?.offsetWidth || Math.min(460, containerRect.width - 16),
      popupHeight: completionPanel.current?.offsetHeight || Math.min(360, containerRect.height - 16),
    });
    setCompletionPosition((previous) => previous.left === next.left && previous.top === next.top && previous.maxHeight === next.maxHeight ? previous : next);
  };

  const updateCompletion = (items: CommandCompletion[], input: string, selected = 0) => {
    const next: CompletionState = items.length ? { items, input, selected: Math.max(0, Math.min(selected, items.length - 1)), selectionActive: false } : emptyCompletion;
    completion.current = next;
    completionPointer.current = null;
    if (items.length) positionCompletion();
    setCompletionState(next);
  };

  const selectCompletion = (index: number) => {
    const state = completion.current;
    if (!state.items.length) return;
    const selected = Math.max(0, Math.min(index, state.items.length - 1));
    if (selected === state.selected && state.selectionActive) return;
    const next = { ...state, selected, selectionActive: true };
    completion.current = next;
    setCompletionState(next);
  };

  const trackCompletionPointer = (position: CompletionPointerPosition) => {
    completionPointer.current = position;
  };

  const selectCompletionFromPointer = (index: number, position: CompletionPointerPosition) => {
    const shouldSelect = shouldSelectCompletionOnPointerMove(completionPointer.current, position);
    completionPointer.current = position;
    if (shouldSelect) selectCompletion(index);
  };

  const closeCompletion = () => updateCompletion([], '');

  const refreshCompletion = (manual = false) => {
    const line = recorder.current?.snapshot();
    if (!completionEnabledCurrent.current || current.current.state !== 'connected' || terminal.current?.buffer.active.type !== 'normal' || !line?.confident || line.cursor !== line.text.length || (!manual && line.text.trim().length < 2)) {
      closeCompletion();
      return;
    }
    updateCompletion(getCommandCompletions(line.text, history.current), line.text);
  };

  const applyCompletion = (index: number) => {
    const item = completion.current.items[index];
    const line = recorder.current?.snapshot();
    const session = current.current;
    if (!item || !line?.confident || line.cursor !== line.text.length || line.text !== completion.current.input || !item.command.startsWith(line.text) || session.state !== 'connected') {
      closeCompletion();
      return;
    }
    const suffix = item.command.slice(line.text.length);
    if (!suffix) return;
    recorder.current?.accept(suffix);
    void desktop.terminalWrite(session.target.sessionId, session.target.generation, Array.from(new TextEncoder().encode(suffix))).catch(() => {});
    closeCompletion();
    terminal.current?.focus();
  };

  useEffect(() => {
    const previous = current.current;
    current.current = session;
    if (previous.target.generation !== session.target.generation || previous.channelId !== session.channelId) {
      recorder.current?.clear();
      closeCompletion();
      if (terminal.current) terminal.current.write(`\r\n\u001b[33m[${t('newShellGeneration')} ${session.target.generation}]\u001b[0m\r\n`);
    }
    if (session.state !== 'connected') closeCompletion();
  }, [session, t]);

  useEffect(() => {
    if (!completionEnabled) closeCompletion();
  }, [completionEnabled]);

  useEffect(() => {
    const eventNode = (event: Event) => event.target instanceof Node ? event.target : null;
    const onPointerDown = (event: PointerEvent) => {
      if (!completion.current.items.length) return;
      if (shouldDismissCompletion(eventNode(event), [completionPanel.current])) closeCompletion();
    };
    const onWorkspaceScroll = (event: Event) => {
      if (!completion.current.items.length) return;
      if (shouldDismissCompletion(eventNode(event), [completionPanel.current])) closeCompletion();
    };
    document.addEventListener('pointerdown', onPointerDown, true);
    document.addEventListener('wheel', onWorkspaceScroll, { capture: true, passive: true });
    document.addEventListener('scroll', onWorkspaceScroll, true);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true);
      document.removeEventListener('wheel', onWorkspaceScroll, true);
      document.removeEventListener('scroll', onWorkspaceScroll, true);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    history.current = [];
    void desktop.commandHistory(session.target.serverId, 100).then((entries) => {
      if (!cancelled) history.current = entries;
    }).catch(() => {});
    return () => { cancelled = true; };
  }, [session.target.serverId]);

  useEffect(() => {
    if (!host.current) return;
    const lineRecorder = new CommandLineRecorder();
    recorder.current = lineRecorder;
    const xterm = new Xterm({ fontFamily, fontSize, theme: terminalThemes[theme], cursorBlink: true, scrollback: 5000, allowProposedApi: false });
    xterm.attachCustomKeyEventHandler((event) => {
      const state = completion.current;
      const action = terminalCompletionKeyAction(event, state.items.length > 0, state.selectionActive, completionEnabledCurrent.current);
      if (action === 'ignore') return false;
      if (action) {
        if (action === 'open') refreshCompletion(true);
        else if (action === 'close') closeCompletion();
        else if (action === 'previous' || action === 'next') {
          const index = state.selectionActive ? getCompletionSelectionIndex(state.selected, state.items.length, action) : action === 'next' ? 0 : state.items.length - 1;
          selectCompletion(index);
        } else applyCompletion(state.selected);
        return false;
      }
      if ((event.key === 'Tab' || event.key === 'Enter') && completion.current.items.length) closeCompletion();
      return !shouldBypassTerminalKeyEvent(event);
    });
    const fit = new FitAddon();
    xterm.loadAddon(fit);
    xterm.open(host.current);
    const paste = (event: Event) => {
      const text = (event as CustomEvent<string>).detail;
      if (typeof text === 'string') xterm.paste(text);
    };
    host.current.addEventListener('sparrowshell-terminal-paste', paste);
    terminal.current = xterm;
    fitAddon.current = fit;
    const frame = requestAnimationFrame(() => {
      if (host.current && host.current.clientWidth > 0) {
        fit.fit();
        const target = current.current.target;
        void desktop.terminalResize(target.sessionId, target.generation, xterm.cols, xterm.rows).catch(() => {});
      }
    });
    const input = xterm.onData((data) => {
      const session = current.current;
      if (session.state !== 'connected') return;
      void desktop.terminalWrite(session.target.sessionId, session.target.generation, Array.from(new TextEncoder().encode(data))).catch(() => {});
      for (const command of lineRecorder.accept(data)) {
        void desktop.recordCommand(session.target.serverId, command).then((entry) => {
          if (entry) history.current = [entry, ...history.current.filter((item) => item.command !== entry.command)].slice(0, 100);
        }).catch(() => {});
      }
      refreshCompletion();
    });
    xterm.onSelectionChange(() => onSelection(xterm.getSelection()));
    const resize = new ResizeObserver(() => {
      if (!host.current || host.current.clientWidth === 0 || host.current.clientHeight === 0) return;
      fit.fit();
      const target = current.current.target;
      void desktop.terminalResize(target.sessionId, target.generation, xterm.cols, xterm.rows).catch(() => {});
      positionCompletion();
    });
    resize.observe(host.current);
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void desktop.onTerminal((event) => {
      const session = current.current;
      if (event.target.sessionId === session.target.sessionId && event.target.generation === session.target.generation && event.channelId === session.channelId) {
        xterm.write(new Uint8Array(event.data), () => {
          if (completion.current.items.length) positionCompletion();
        });
      }
    }).then((dispose) => {
      if (disposed) dispose(); else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
      cancelAnimationFrame(frame);
      input.dispose();
      resize.disconnect();
      host.current?.removeEventListener('sparrowshell-terminal-paste', paste);
      xterm.dispose();
      terminal.current = null;
      fitAddon.current = null;
      recorder.current = null;
      completion.current = emptyCompletion;
    };
  }, []);

  useEffect(() => {
    if (!active) return;
    const frame = requestAnimationFrame(() => {
      const xterm = terminal.current;
      if (!xterm || !fitAddon.current || !host.current || host.current.clientWidth === 0) return;
      fitAddon.current.fit();
      const target = current.current.target;
      void desktop.terminalResize(target.sessionId, target.generation, xterm.cols, xterm.rows).catch(() => {});
      xterm.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [active]);

  useEffect(() => {
    const xterm = terminal.current;
    if (!xterm) return;
    xterm.options.fontFamily = fontFamily;
    xterm.options.fontSize = fontSize;
    const frame = requestAnimationFrame(() => {
      if (!fitAddon.current || !host.current || host.current.clientWidth === 0) return;
      fitAddon.current.fit();
      const target = current.current.target;
      void desktop.terminalResize(target.sessionId, target.generation, xterm.cols, xterm.rows).catch(() => {});
    });
    return () => cancelAnimationFrame(frame);
  }, [fontFamily, fontSize]);

  useEffect(() => {
    if (terminal.current) terminal.current.options.theme = terminalThemes[theme];
  }, [theme]);

  useLayoutEffect(() => {
    if (completionState.items.length) positionCompletion();
  }, [completionState.items, completionState.input, fontFamily, fontSize, active]);

  useEffect(() => {
    completionList.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [completionState.selected]);

  return <section className="terminal-instance" ref={instance} style={{ display: active ? 'flex' : 'none' }} aria-hidden={!active}>
    <div className="terminal-toolbar"><button className={`terminal-completion-toggle ${completionEnabled ? 'active' : ''}`} title={`${t(completionEnabled ? 'disableCompletion' : 'enableCompletion')} · Ctrl + Space\n${t('completionSources')}`} aria-label={t(completionEnabled ? 'disableCompletion' : 'enableCompletion')} aria-pressed={completionEnabled} disabled={!active} onClick={() => { onCompletionEnabledChange(!completionEnabled); if (completionEnabled) closeCompletion(); terminal.current?.focus(); }}>{t(completionEnabled ? 'disableCompletion' : 'enableCompletion')}</button><select className="terminal-font-select" title={t('terminalFont')} aria-label={t('terminalFont')} value={fontFamily} disabled={!active} onChange={(event) => onFontFamilyChange(event.target.value)}>{terminalFonts.map((font) => <option key={font.label} value={font.value}>{font.label}</option>)}</select><TerminalFontSizeInput value={fontSize} onChange={onFontSizeChange} className="terminal-font-size" aria-label={t('fontSize')} /><span className="encoding">UTF-8</span><button className="icon small" title={t('interruptCommand')} aria-label={t('interruptCommand')} disabled={!active || session.state !== 'connected'} onClick={() => { const target = current.current.target; void desktop.terminalWrite(target.sessionId, target.generation, [3]).catch(() => {}); recorder.current?.clear(); closeCompletion(); terminal.current?.focus(); }}><Square size={13} /></button><button className="icon small" title={t('commandHistory')} aria-label={t('commandHistory')} disabled={!active} onClick={onHistory}><Clock3 size={14} /></button><button className="icon small" title={t('reconnect')} aria-label={t('reconnect')} disabled={!active} onClick={onReconnect}><RotateCcw size={14} /></button><button className="icon small" title={t('clearTerminal')} aria-label={t('clearTerminal')} disabled={!active} onClick={() => terminal.current?.clear()}><Trash2 size={14} /></button></div>
    <div className="terminal-canvas" ref={host} aria-label="SSH terminal" />
    {active && completionEnabled && completionState.items.length > 0 && <section className="terminal-completion" ref={completionPanel} style={{ left: completionPosition.left, top: completionPosition.top, maxHeight: completionPosition.maxHeight }} aria-label={t('smartCompletion')}>
      <header><span><Sparkles size={13} />{t('smartCompletion')}</span><kbd>Ctrl + Space</kbd><button className="icon small" aria-label={t('close')} onMouseDown={(event) => event.preventDefault()} onClick={() => { closeCompletion(); terminal.current?.focus(); }}><X size={13} /></button></header>
      <div className="terminal-completion-list" role="listbox" ref={completionList}>{completionState.items.map((item, index) => <button type="button" role="option" aria-selected={completionState.selectionActive && index === completionState.selected} className={completionState.selectionActive && index === completionState.selected ? 'selected' : ''} key={`${item.source}:${item.command}`} onMouseDown={(event) => event.preventDefault()} onMouseEnter={(event) => trackCompletionPointer({ x: event.clientX, y: event.clientY })} onMouseMove={(event) => selectCompletionFromPointer(index, { x: event.clientX, y: event.clientY })} onClick={() => applyCompletion(index)}>
        <div className="completion-command"><code><b>{item.command.slice(0, completionState.input.length)}</b>{item.command.slice(completionState.input.length)}</code>{item.highRisk && <em>{t('highRisk')}</em>}</div>
        <div className="completion-detail"><span>{item.description?.[locale] ?? t('fillOnly')}</span><small>{t(item.source === 'history' ? 'completionHistory' : 'completionCommon')}</small></div>
      </button>)}</div>
      <footer>{t(completionState.selectionActive ? 'completionHintSelected' : 'completionHint')}</footer>
    </section>}
  </section>;
}
