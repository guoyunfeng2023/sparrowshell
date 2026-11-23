import type { AiContextData, AiContextPermissions, ExecutionRecord, SystemSnapshot } from '../../types';

export function selectAuthorizedContext(data: AiContextData, permissions: AiContextPermissions): AiContextData {
  const selected: AiContextData = {};
  for (const key of Object.keys(permissions) as (keyof AiContextPermissions)[]) {
    if (permissions[key] && data[key] !== undefined) Object.assign(selected, { [key]: data[key] });
  }
  return selected;
}

/** Minimal system facts only. Process command lines are deliberately excluded. */
export function systemContext(snapshot: SystemSnapshot): string {
  return JSON.stringify({
    collectedAt: snapshot.collectedAt,
    hostname: snapshot.hostname,
    osName: snapshot.osName,
    kernel: snapshot.kernel,
    architecture: snapshot.architecture,
    uptimeSeconds: snapshot.uptimeSeconds,
    loadAverage: snapshot.loadAverage,
    cpuUsagePercent: snapshot.cpuUsagePercent,
    memoryTotalBytes: snapshot.memoryTotalBytes,
    memoryUsedBytes: snapshot.memoryUsedBytes,
    disks: snapshot.disks,
  });
}

export function recentExecutionContext(executions: Record<string, ExecutionRecord>): string[] {
  return Object.values(executions)
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt))
    .slice(0, 5)
    .map((execution) => JSON.stringify({
      command: execution.command,
      state: execution.state,
      exitStatus: execution.exitStatus,
      stdout: execution.stdout,
      stderr: execution.stderr,
    }));
}
