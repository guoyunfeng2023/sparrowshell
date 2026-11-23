import { describe, expect, it } from 'vitest';
import { buildBatchSpec } from './batchSpec';
import type { TargetRef } from '../../types';

const target: TargetRef = { serverId: 'server-1', sessionId: 'session-1', generation: 3 };

describe('batch execution mode', () => {
  it('always uses auto_all without requiring command approval', () => {
    expect(buildBatchSpec('uptime', [target], 2)).toEqual({
      command: 'uptime',
      targets: [target],
      maxConcurrent: 2,
      mode: 'auto_all',
    });
  });
});
