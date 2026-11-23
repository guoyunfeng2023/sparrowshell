import { describe, expect, it } from 'vitest';
import { recentExecutionContext, systemContext } from './context';
import type { ExecutionRecord, SystemSnapshot, TargetRef } from '../../types';

const target: TargetRef = { serverId: 'server', sessionId: 'session', generation: 1 };
const snapshot: SystemSnapshot = {
  target, collectedAt: '2026-10-29T13:00:00Z', hostname: 'host', osName: 'Linux', kernel: '6.8', architecture: 'x86_64', cpuCores: 4,
  uptimeSeconds: 100, loadAverage: [0.1, 0.2, 0.3], cpuUsagePercent: 12, memoryTotalBytes: 2000, memoryUsedBytes: 1000,
  disks: [{ filesystem: '/dev/test', mountPoint: '/', totalBytes: 2000, usedBytes: 500, availableBytes: 1500 }],
  processes: [{ pid: 1, ppid: 0, user: 'root', cpuPercent: 1, memoryPercent: 1, startId: '1', command: 'process-must-not-be-sent' }],
};

describe('authorized AI context minimization', () => {
  it('contains system and disk facts without process command lines or target metadata', () => {
    const context = systemContext(snapshot);
    expect(context).toContain('hostname');
    expect(context).toContain('mountPoint');
    expect(context).not.toContain('process-must-not-be-sent');
    expect(context).not.toContain('sessionId');
  });

  it('uses at most five latest real AI execution records without inventing output', () => {
    const records = Object.fromEntries(Array.from({ length: 7 }, (_, index) => {
      const id = `execution-${index}`;
      const record: ExecutionRecord = {
        executionId: id, source: 'ai', target, mode: 'confirm_each', command: `command-${index}`, commandHash: `hash-${index}`,
        risk: { level: 'low', reasons: [] }, state: 'succeeded', approvalId: null, approvalExpiresAt: null,
        stdout: `real-output-${index}`, stderr: '', exitStatus: 0, errorCode: null,
        createdAt: `2026-10-29T13:00:0${index}Z`, startedAt: null, finishedAt: null,
      };
      return [id, record];
    }));
    const results = recentExecutionContext(records);
    expect(results).toHaveLength(5);
    expect(results[0]).toContain('command-6');
    expect(results[0]).toContain('real-output-6');
    expect(results.join('')).not.toContain('command-0');
  });
});
