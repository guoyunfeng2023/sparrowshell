import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';
import type { MessageKey } from '../../i18n';
import { desktop, isDesktop } from '../../ipc/client';

type Translate = (key: MessageKey) => string;
type ContextAction = 'cut' | 'copy' | 'paste' | 'selectAll';

interface MenuState {
  x: number;
  y: number;
  target: HTMLElement;
  editable: boolean;
  hasSelection: boolean;
}

function isEditable(target: HTMLElement): target is HTMLInputElement | HTMLTextAreaElement {
  return target instanceof HTMLTextAreaElement
    || (target instanceof HTMLInputElement && !['button', 'checkbox', 'radio', 'range', 'submit', 'reset'].includes(target.type))
    || target.isContentEditable;
}

function selectionIn(target: HTMLElement): string {
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
    return target.value.slice(target.selectionStart ?? 0, target.selectionEnd ?? 0);
  }
  return window.getSelection()?.toString() ?? '';
}

export function LocalizedContextMenu({ className, children, t }: { className: string; children: ReactNode; t: Translate }) {
  const [menu, setMenu] = useState<MenuState | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const closeOutside = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) close();
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close();
    };
    document.addEventListener('pointerdown', closeOutside);
    document.addEventListener('keydown', closeOnEscape);
    window.addEventListener('resize', close);
    window.addEventListener('blur', close);
    return () => {
      document.removeEventListener('pointerdown', closeOutside);
      document.removeEventListener('keydown', closeOnEscape);
      window.removeEventListener('resize', close);
      window.removeEventListener('blur', close);
    };
  }, [menu]);

  const open = (event: globalThis.MouseEvent) => {
    event.preventDefault();
    const target = event.target instanceof HTMLElement ? event.target : document.body;
    if (menuRef.current?.contains(target)) return;
    const editableTarget = target.closest('input, textarea, [contenteditable="true"]') as HTMLElement | null;
    const activeElement = document.activeElement instanceof HTMLElement && isEditable(document.activeElement) ? document.activeElement : null;
    const terminalInput = target.closest('.terminal-instance')?.querySelector('textarea') ?? null;
    const actionTarget = editableTarget ?? activeElement ?? terminalInput ?? target;
    setMenu({
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - 164)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - 154)),
      target: actionTarget,
      editable: actionTarget ? isEditable(actionTarget) : false,
      hasSelection: selectionIn(actionTarget).length > 0,
    });
  };

  useEffect(() => {
    document.addEventListener('contextmenu', open);
    return () => document.removeEventListener('contextmenu', open);
  }, []);

  const run = async (action: ContextAction) => {
    if (!menu) return;
    const { target } = menu;
    target.focus({ preventScroll: true });
    if (action === 'selectAll') {
      document.execCommand('selectAll');
    } else if (action === 'paste') {
      const text = isDesktop ? await desktop.readClipboard() : await navigator.clipboard.readText();
      if (target.closest('.terminal-instance')) {
        target.dispatchEvent(new CustomEvent('sparrowshell-terminal-paste', { detail: text, bubbles: true }));
      } else if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
        const start = target.selectionStart ?? target.value.length;
        const end = target.selectionEnd ?? start;
        const value = `${target.value.slice(0, start)}${text}${target.value.slice(end)}`;
        const prototype = target instanceof HTMLInputElement ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
        Object.getOwnPropertyDescriptor(prototype, 'value')?.set?.call(target, value);
        target.setSelectionRange(start + text.length, start + text.length);
        target.dispatchEvent(new Event('input', { bubbles: true }));
      } else {
        document.execCommand('insertText', false, text);
      }
    } else if (action === 'copy') {
      if (!document.execCommand('copy')) await navigator.clipboard.writeText(selectionIn(target));
    } else {
      document.execCommand('cut');
    }
    setMenu(null);
  };

  const item = (action: ContextAction, label: string, disabled = false) => <button
    type="button"
    role="menuitem"
    disabled={disabled}
    onMouseDown={(event) => event.preventDefault()}
    onClick={() => void run(action)}
  >{label}</button>;

  return <div className={className}>
    {children}
    {menu && createPortal(<div
      ref={menuRef}
      className="localized-context-menu"
      role="menu"
      aria-label={t('contextMenu')}
      style={{ left: menu.x, top: menu.y }}
      onContextMenu={(event) => event.preventDefault()}
    >
      {item('cut', t('contextCut'), !menu.editable || !menu.hasSelection)}
      {item('copy', t('contextCopy'), !menu.hasSelection)}
      {item('paste', t('contextPaste'))}
      <div className="menu-divider" />
      {item('selectAll', t('contextSelectAll'))}
    </div>, document.body)}
  </div>;
}
