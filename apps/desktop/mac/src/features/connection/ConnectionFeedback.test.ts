import { describe, expect, it } from 'vitest';
import { connectionPhaseKey, isConnectingState } from './ConnectionFeedback';

describe('connection feedback', () => {
  it.each(['resolving', 'verifying_target', 'authenticating'] as const)('keeps the UI busy during %s', (state) => {
    expect(isConnectingState(state)).toBe(true);
  });

  it.each(['connected', 'disconnected', 'host_key_blocked', 'error'] as const)('does not report %s as connecting', (state) => {
    expect(isConnectingState(state)).toBe(false);
  });

  it('uses real connection states for phase feedback', () => {
    expect(connectionPhaseKey(undefined)).toBe('connectPhasePrepare');
    expect(connectionPhaseKey('resolving')).toBe('connectPhaseNetwork');
    expect(connectionPhaseKey('verifying_target')).toBe('connectPhaseNetwork');
    expect(connectionPhaseKey('authenticating')).toBe('connectPhaseAuth');
  });
});
