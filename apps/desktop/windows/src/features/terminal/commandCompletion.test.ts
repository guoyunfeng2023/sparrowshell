import { describe, expect, it } from 'vitest';
import type { CommandHistoryEntry } from '../../types';
import { getCommandCompletions, isHighRiskCompletion } from './commandCompletion';

function history(command: string, id = command): CommandHistoryEntry {
  return { id, serverId: 'server-1', command, createdAt: '2026-11-12T00:00:00Z' };
}

describe('command completion', () => {
  it('suggests common command prefixes with bilingual descriptions', () => {
    const items = getCommandCompletions('systemctl st', []);
    expect(items[0]).toMatchObject({ command: 'systemctl status', source: 'common' });
    expect(items[0].description).toMatchObject({ zh: '查看系统或服务状态' });
    expect(items.some((item) => item.command === 'systemctl start')).toBe(true);
    expect(items.find((item) => item.command === 'systemctl start')?.description?.zh).toContain('需补充服务名');
  });

  it('suggests ZIP commands for the un prefix', () => {
    const items = getCommandCompletions('un', []);
    expect(items.map((item) => item.command)).toEqual(expect.arrayContaining(['uname -a', 'unzip', 'unzip -l']));
    expect(items.find((item) => item.command === 'unzip')).toMatchObject({
      source: 'common',
      description: { zh: '解压 ZIP（需补充压缩包路径）', en: 'Extract ZIP (add an archive path)' },
    });
  });

  it('keeps parameter guidance when a built-in command also exists in history', () => {
    const items = getCommandCompletions('unzip', [history('unzip -l')]);
    expect(items[0]).toMatchObject({
      command: 'unzip -l',
      source: 'history',
      description: { zh: '查看 ZIP 内容（需补充压缩包路径）', en: 'List ZIP contents (add an archive path)' },
    });
  });

  it('prioritizes recent local history and removes duplicates', () => {
    const items = getCommandCompletions('docker p', [history('docker ps --format json'), history('docker ps')]);
    expect(items.map((item) => item.command)).toEqual(['docker ps --format json', 'docker ps', 'docker ps -a']);
    expect(new Set(items.map((item) => item.command)).size).toBe(items.length);
  });

  it('never returns the unchanged command or more than the requested limit', () => {
    expect(getCommandCompletions('git status', [])).toEqual([]);
    expect(getCommandCompletions('', [], 8)).toHaveLength(8);
  });

  it('rejects multiline, indented and potentially sensitive input', () => {
    expect(getCommandCompletions('  docker', [])).toEqual([]);
    expect(getCommandCompletions('docker\nps', [])).toEqual([]);
    expect(getCommandCompletions('export', [history('export API_KEY=secret'), history('export PATH=/usr/local/bin:$PATH')]).map((item) => item.command)).toEqual(['export PATH=/usr/local/bin:$PATH']);
  });

  it('marks destructive suggestions without executing or changing them', () => {
    expect(isHighRiskCompletion('rm -rf /tmp/example')).toBe(true);
    expect(isHighRiskCompletion('docker compose down')).toBe(true);
    expect(isHighRiskCompletion('docker compose ps')).toBe(false);
    const item = getCommandCompletions('docker compose d', [])[0];
    expect(item).toMatchObject({ command: 'docker compose down', highRisk: true });
  });
});
