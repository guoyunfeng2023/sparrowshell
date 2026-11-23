import { describe, expect, it } from 'vitest';
import { summarizeBatchProgress } from './batchProgress';
import type { TargetRun, TargetRunState } from '../../types';

const states: TargetRunState[] = ['queued', 'running', 'succeeded', 'failed', 'stopped', 'unknown'];
const targets: TargetRun[] = states.map((state, index) => ({
  target: { serverId: `server-${index}`, sessionId: `session-${index}`, generation: 1 },
  state,
  executionId: null,
  exitStatus: null,
  errorCode: null,
  stdout: '',
  stderr: '',
}));

describe('batch progress', () => {
  it('counts every target state and only terminal states as completed', () => {
    expect(summarizeBatchProgress({ targets })).toEqual({
      total: 6,
      completed: 4,
      queued: 1,
      running: 1,
      succeeded: 1,
      failed: 1,
      stopped: 1,
      unknown: 1,
    });
  });

  it('does not count queued or running targets as completed', () => {
    const progress = summarizeBatchProgress({ targets: targets.slice(0, 2) });
    expect(progress.total).toBe(2);
    expect(progress.completed).toBe(0);
  });
});
