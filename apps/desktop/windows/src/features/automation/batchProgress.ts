import type { BatchRun } from '../../types';

export interface BatchProgress {
  total: number;
  completed: number;
  queued: number;
  running: number;
  succeeded: number;
  failed: number;
  stopped: number;
  unknown: number;
}

export function summarizeBatchProgress(run: Pick<BatchRun, 'targets'>): BatchProgress {
  const progress: BatchProgress = {
    total: run.targets.length,
    completed: 0,
    queued: 0,
    running: 0,
    succeeded: 0,
    failed: 0,
    stopped: 0,
    unknown: 0,
  };
  for (const target of run.targets) {
    progress[target.state] += 1;
    if (target.state !== 'queued' && target.state !== 'running') progress.completed += 1;
  }
  return progress;
}
