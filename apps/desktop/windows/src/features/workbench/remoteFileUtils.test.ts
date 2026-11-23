import { describe, expect, it } from 'vitest';
import { contextMenuPosition, formatRemoteModified, isDragPositionInside, isRemoteArchiveConflict, isSupportedArchive, localFileName, normalizeDragPosition, parentRemotePath, platformReportsPhysicalDragPixels, remoteArchiveConflictPath } from './remoteFileUtils';

describe('archive extraction conflicts', () => {
  it('recognizes only the explicit non-overwrite conflict', () => {
    expect(isRemoteArchiveConflict(new Error('远程目标已存在，未覆盖'))).toBe(true);
    expect(isRemoteArchiveConflict('远程目标已存在，未覆盖')).toBe(true);
    expect(isRemoteArchiveConflict(new Error('压缩包无法读取或已损坏'))).toBe(false);
  });

  it('extracts the exact remote path returned by the conflict check', () => {
    expect(remoteArchiveConflictPath(new Error('解压目标已存在：/srv/app/config/application.yml'))).toBe('/srv/app/config/application.yml');
    expect(remoteArchiveConflictPath(new Error('解压目标已存在：/srv/app/'))).toBe('/srv/app/');
    expect(remoteArchiveConflictPath(new Error('远程目标已存在，未覆盖'))).toBe('');
    expect(remoteArchiveConflictPath(new Error('解压失败：其他错误'))).toBeNull();
  });
});

describe('supported remote archives', () => {
  it('supports ZIP and TAR formats case-insensitively', () => {
    for (const name of ['app.zip', 'APP.ZIP', 'app.tar', 'app.tar.gz', 'app.TAR.GZ', 'app.tgz', 'app.TGZ']) {
      expect(isSupportedArchive(name)).toBe(true);
    }
  });

  it('does not treat unsupported or merely similar names as archives', () => {
    for (const name of ['app.rar', 'app.7z', 'app.gz', 'app.zip.txt', 'archive.tar.gz.backup', 'zip']) {
      expect(isSupportedArchive(name)).toBe(false);
    }
  });
});

describe('remote file paths', () => {
  it('keeps root and nested parent paths POSIX-correct', () => {
    expect(parentRemotePath('/etc/nginx/nginx.conf')).toBe('/etc/nginx');
    expect(parentRemotePath('/nginx.conf')).toBe('/');
    expect(parentRemotePath('relative.conf')).toBe('/');
  });
});

describe('remote context menu positioning', () => {
  it('keeps the whole menu inside the viewport', () => {
    expect(contextMenuPosition(900, 700, 190, 360, 1000, 800)).toEqual({ x: 802, y: 432 });
    expect(contextMenuPosition(0, 0, 190, 360, 1000, 800)).toEqual({ x: 8, y: 8 });
  });
});

describe('Finder drag coordinates', () => {
  const rect = { left: 800, top: 100, right: 1200, bottom: 700 };

  it('uses macOS logical points without dividing them again on Retina displays', () => {
    expect(isDragPositionInside({ x: 1000, y: 300 }, rect)).toBe(true);
    expect(isDragPositionInside({ x: 2000, y: 600 }, rect)).toBe(false);
  });

  it('rejects points outside every panel edge', () => {
    expect(isDragPositionInside({ x: 799, y: 200 }, rect)).toBe(false);
    expect(isDragPositionInside({ x: 900, y: 701 }, rect)).toBe(false);
    expect(isDragPositionInside({ x: 1200, y: 700 }, rect)).toBe(true);
  });
});

describe('Windows drag position normalization', () => {
  it('scales physical WebView2 pixels back to CSS coordinates', () => {
    // 150% 缩放：物理 1500,450 → CSS 1000,300
    expect(normalizeDragPosition({ x: 1500, y: 450 }, 1.5, true)).toEqual({ x: 1000, y: 300 });
    // 125% 缩放：物理 1250,375 → CSS 1000,300
    expect(normalizeDragPosition({ x: 1250, y: 375 }, 1.25, true)).toEqual({ x: 1000, y: 300 });
  });

  it('keeps macOS logical points untouched even on Retina displays', () => {
    expect(normalizeDragPosition({ x: 1000, y: 300 }, 2, false)).toEqual({ x: 1000, y: 300 });
  });

  it('is a no-op at 100% scaling or invalid scale factors', () => {
    expect(normalizeDragPosition({ x: 1000, y: 300 }, 1, true)).toEqual({ x: 1000, y: 300 });
    expect(normalizeDragPosition({ x: 1000, y: 300 }, 0, true)).toEqual({ x: 1000, y: 300 });
    expect(normalizeDragPosition({ x: 1000, y: 300 }, Number.NaN, true)).toEqual({ x: 1000, y: 300 });
  });

  it('detects Windows WebView2 from the user agent', () => {
    expect(platformReportsPhysicalDragPixels('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 EdgW/141.0')).toBe(true);
    expect(platformReportsPhysicalDragPixels('Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0) AppleWebKit/605.1.15 (KHTML, like Gecko)')).toBe(false);
  });

  it('normalized Windows coordinates land inside the panel rect', () => {
    const rect = { left: 800, top: 100, right: 1200, bottom: 700 };
    // 用户在 150% 缩放下把文件拖到面板视觉中心：CSS(1000,400) → 物理(1500,600)
    const normalized = normalizeDragPosition({ x: 1500, y: 600 }, 1.5, true);
    expect(isDragPositionInside(normalized, rect)).toBe(true);
  });
});

describe('local file names', () => {
  it('extracts the file name from Windows paths with backslashes', () => {
    expect(localFileName('C:\\Users\\HP\\Desktop\\publish.zip')).toBe('publish.zip');
    expect(localFileName('D:\\归档\\数据备份.zip')).toBe('数据备份.zip');
    expect(localFileName('\\\\server\\share\\release.tar.gz')).toBe('release.tar.gz');
  });

  it('extracts the file name from POSIX paths', () => {
    expect(localFileName('/Users/hp/Desktop/publish.zip')).toBe('publish.zip');
    expect(localFileName('publish.zip')).toBe('publish.zip');
  });

  it('rejects paths without a file name', () => {
    expect(localFileName('C:\\')).toBeUndefined();
    expect(localFileName('/')).toBeUndefined();
    expect(localFileName('')).toBeUndefined();
  });
});

describe('remote modification time', () => {
  it('formats valid timestamps as local date and minute', () => {
    expect(formatRemoteModified(0)).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
  });

  it('shows a placeholder when the server did not provide a valid time', () => {
    expect(formatRemoteModified(null)).toBe('—');
    expect(formatRemoteModified(Number.NaN)).toBe('—');
    expect(formatRemoteModified(Number.POSITIVE_INFINITY)).toBe('—');
  });
});
