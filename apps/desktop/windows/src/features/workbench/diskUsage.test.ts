import { describe, expect, it } from 'vitest';
import type { DiskUsage } from '../../types';
import { diskUsagePercent, selectDisks, sortDisksByUsage } from './diskUsage';

function disk(mountPoint: string, usedBytes: number, totalBytes: number, filesystem = `/dev/${mountPoint}`): DiskUsage {
  return { filesystem, mountPoint, usedBytes, totalBytes, availableBytes: totalBytes - usedBytes };
}

describe('disk usage ordering', () => {
  it('keeps every disk and sorts usage from highest to lowest', () => {
    const disks = [disk('/low', 10, 100), disk('/high', 90, 100), disk('/middle', 50, 100), disk('/fourth', 20, 100)];

    expect(sortDisksByUsage(disks).map((item) => item.mountPoint)).toEqual(['/high', '/middle', '/fourth', '/low']);
    expect(sortDisksByUsage(disks)).toHaveLength(4);
  });

  it('preserves the original order for equal usage and does not mutate the input', () => {
    const disks = [disk('/first', 25, 100), disk('/second', 50, 200), disk('/third', 10, 100)];

    expect(sortDisksByUsage(disks).map((item) => item.mountPoint)).toEqual(['/first', '/second', '/third']);
    expect(disks.map((item) => item.mountPoint)).toEqual(['/first', '/second', '/third']);
  });

  it('handles invalid totals and clamps the displayed percentage', () => {
    expect(diskUsagePercent(disk('/invalid', 10, 0))).toBe(0);
    expect(diskUsagePercent(disk('/full', 120, 100))).toBe(100);
    expect(sortDisksByUsage([disk('/invalid', 10, 0), disk('/used', 10, 100)]).map((item) => item.mountPoint)).toEqual(['/used', '/invalid']);
  });
});

describe('disk view selection', () => {
  const disks = [
    disk('/var/lib/docker/overlay2/longer', 40, 100, '/dev/vda1'),
    disk('/', 40, 100, '/dev/vda1'),
    disk('/dev/shm', 0, 100, 'tmpfs'),
    disk('/run', 5, 100, 'tmpfs'),
    disk('/boot', 20, 100, '/dev/vda2'),
  ];

  it('keeps the original filtered and deduplicated partition view by default', () => {
    expect(selectDisks(disks, false).map((item) => item.mountPoint)).toEqual(['/', '/boot']);
  });

  it('keeps temporary filesystems and every mount point in all-mounts view', () => {
    expect(selectDisks(disks, true).map((item) => item.mountPoint)).toEqual(disks.map((item) => item.mountPoint));
    expect(selectDisks(disks, true)).not.toBe(disks);
  });
});
