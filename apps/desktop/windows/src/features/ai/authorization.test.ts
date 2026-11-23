import { describe, expect, it } from 'vitest';
import { selectAuthorizedContext } from './context';
import type { AiContextData, AiContextPermissions } from '../../types';

const data: AiContextData = {
  serverLabel: 'production', systemInfo: '{"cpu":12}', workingDirectory: '/var/log',
  selectedOutput: 'selected text', recentResults: ['actual result'], directoryEntries: ['nginx'],
};
const denied: AiContextPermissions = {
  serverLabel: false, systemInfo: false, workingDirectory: false,
  selectedOutput: false, recentResults: false, directoryEntries: false,
};

describe('per-category AI authorization', () => {
  it('sends no server data by default', () => {
    expect(selectAuthorizedContext(data, denied)).toEqual({});
  });

  it('selects only explicitly granted categories and preserves the input', () => {
    const selected = selectAuthorizedContext(data, { ...denied, systemInfo: true, directoryEntries: true });
    expect(selected).toEqual({ systemInfo: '{"cpu":12}', directoryEntries: ['nginx'] });
    expect(data.selectedOutput).toBe('selected text');
  });
});
