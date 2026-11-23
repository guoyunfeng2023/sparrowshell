import type { BatchSpec, TargetRef } from '../../types';

export function buildBatchSpec(command: string, targets: TargetRef[], maxConcurrent: number): BatchSpec {
  return { command, targets, maxConcurrent, mode: 'auto_all' };
}
