import { useEffect, useState } from 'react';

export const terminalFonts = [
  { label: 'SF Mono', value: 'SFMono-Regular, Menlo, monospace' },
  { label: '更纱黑体 SC', value: "'Sarasa Mono SC', 'Sarasa Term SC', SFMono-Regular, 'PingFang SC', monospace" },
  { label: '霞鹜文楷等宽', value: "'LXGW WenKai Mono', SFMono-Regular, 'PingFang SC', monospace" },
  { label: '思源等宽', value: "'Source Han Mono SC', 'Noto Sans Mono CJK SC', SFMono-Regular, 'PingFang SC', monospace" },
  { label: 'Noto 中文等宽', value: "'Noto Sans Mono CJK SC', 'Source Han Mono SC', SFMono-Regular, 'PingFang SC', monospace" },
  { label: 'Maple Mono 中文', value: "'Maple Mono CN', 'Maple Mono NF CN', SFMono-Regular, 'PingFang SC', monospace" },
  { label: '苹方', value: "'PingFang SC', SFMono-Regular, Menlo, monospace" },
  { label: '冬青黑体', value: "'Hiragino Sans GB', 'PingFang SC', SFMono-Regular, monospace" },
  { label: 'JetBrains Mono', value: "'JetBrains Mono', SFMono-Regular, Menlo, monospace" },
  { label: 'Fira Code', value: "'Fira Code', SFMono-Regular, Menlo, monospace" },
  { label: 'Cascadia Code', value: "'Cascadia Code', SFMono-Regular, Menlo, monospace" },
  { label: 'Source Code Pro', value: "'Source Code Pro', SFMono-Regular, Menlo, monospace" },
  { label: 'IBM Plex Mono', value: "'IBM Plex Mono', SFMono-Regular, Menlo, monospace" },
  { label: 'Hack', value: 'Hack, SFMono-Regular, Menlo, monospace' },
  { label: 'Inconsolata', value: 'Inconsolata, SFMono-Regular, Menlo, monospace' },
  { label: 'Roboto Mono', value: "'Roboto Mono', SFMono-Regular, Menlo, monospace" },
  { label: 'Ubuntu Mono', value: "'Ubuntu Mono', SFMono-Regular, Menlo, monospace" },
  { label: 'Noto Sans Mono', value: "'Noto Sans Mono', SFMono-Regular, Menlo, monospace" },
  { label: 'Anonymous Pro', value: "'Anonymous Pro', SFMono-Regular, Menlo, monospace" },
  { label: 'PT Mono', value: "'PT Mono', SFMono-Regular, Menlo, monospace" },
  { label: 'Menlo', value: 'Menlo, Monaco, monospace' },
  { label: 'Monaco', value: 'Monaco, Menlo, monospace' },
  { label: 'Andale Mono', value: "'Andale Mono', Menlo, monospace" },
  { label: 'Courier New', value: "'Courier New', Courier, monospace" },
  { label: 'Liberation Mono', value: "'Liberation Mono', 'Courier New', monospace" },
] as const;

export function TerminalFontSizeInput({ value, onChange, className, 'aria-label': ariaLabel }: { value: number; onChange: (value: number) => void; className?: string; 'aria-label'?: string }) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const parsed = Number(draft);
    const next = draft.trim() && Number.isFinite(parsed) ? Math.min(24, Math.max(10, parsed)) : value;
    setDraft(String(next));
    onChange(next);
  };
  return <input type="number" min={10} max={24} step={1} value={draft} className={className} aria-label={ariaLabel} onChange={(event) => {
    setDraft(event.target.value);
    const parsed = event.target.valueAsNumber;
    if (Number.isFinite(parsed) && parsed >= 10 && parsed <= 24) onChange(parsed);
  }} onBlur={commit} onKeyDown={(event) => { if (event.key === 'Enter') event.currentTarget.blur(); }} />;
}
