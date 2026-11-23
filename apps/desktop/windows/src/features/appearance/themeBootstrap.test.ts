import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const bootstrap = readFileSync(fileURLToPath(new URL('../../../public/theme-bootstrap.js', import.meta.url)), 'utf8');

function applyBootstrap(storage: Record<string, string>) {
  const root = { dataset: {} as Record<string, string>, style: { backgroundColor: '', colorScheme: '' } };
  let themeColor = '';
  const document = {
    documentElement: root,
    querySelector: (selector: string) => selector === 'meta[name="theme-color"]'
      ? { setAttribute: (_name: string, value: string) => { themeColor = value; } }
      : null,
  };
  const localStorage = { getItem: (key: string) => storage[key] ?? null };
  new Function('localStorage', 'document', bootstrap)(localStorage, document);
  return { ...root, themeColor };
}

describe('startup theme bootstrap', () => {
  it('applies the saved light theme before the application renders', () => {
    const result = applyBootstrap({ 'sparrowshell.desktop.preferences': JSON.stringify({ theme: 'light' }) });
    expect(result.dataset.theme).toBe('light');
    expect(result.style).toEqual({ backgroundColor: '#ffffff', colorScheme: 'light' });
    expect(result.themeColor).toBe('#ffffff');
  });

  it('uses the isolated development preference when it exists', () => {
    const result = applyBootstrap({
      'sparrowshell.desktop.development.preferences': JSON.stringify({ theme: 'light' }),
      'sparrowshell.desktop.preferences': JSON.stringify({ theme: 'dark' }),
    });
    expect(result.dataset.theme).toBe('light');
  });

  it('keeps the dark default for missing, malformed or unknown preferences', () => {
    for (const value of [null, '{broken', JSON.stringify({ theme: 'blue' })]) {
      const result = applyBootstrap(value === null ? {} : { 'sparrowshell.desktop.preferences': value });
      expect(result.dataset.theme).toBe('dark');
      expect(result.style.backgroundColor).toBe('#0d1012');
      expect(result.style.colorScheme).toBe('dark');
    }
  });
});
