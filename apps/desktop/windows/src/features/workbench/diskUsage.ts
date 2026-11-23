import type { DiskUsage } from '../../types';

const temporaryFilesystems = new Set(['tmpfs', 'devtmpfs', 'udev', 'shm']);

export function diskUsagePercent(disk: Pick<DiskUsage, 'usedBytes' | 'totalBytes'>): number {
  if (!Number.isFinite(disk.totalBytes) || disk.totalBytes <= 0 || !Number.isFinite(disk.usedBytes)) return 0;
  return Math.min(100, Math.max(0, disk.usedBytes / disk.totalBytes * 100));
}

export function sortDisksByUsage(disks: DiskUsage[]): DiskUsage[] {
  return [...disks].sort((left, right) => diskUsagePercent(right) - diskUsagePercent(left));
}

export function selectDisks(disks: DiskUsage[], showAllMounts: boolean): DiskUsage[] {
  if (showAllMounts) return [...disks];

  const partitions: DiskUsage[] = [];
  for (const candidate of disks) {
    if (temporaryFilesystems.has(candidate.filesystem)) continue;
    const existing = partitions.find((disk) => disk.filesystem === candidate.filesystem && disk.totalBytes === candidate.totalBytes);
    if (!existing) {
      partitions.push(candidate);
    } else if (candidate.mountPoint.length < existing.mountPoint.length) {
      partitions[partitions.indexOf(existing)] = candidate;
    }
  }
  return partitions;
}
