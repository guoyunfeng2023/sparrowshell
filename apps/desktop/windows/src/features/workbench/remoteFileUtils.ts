import type { MessageKey } from '../../i18n';

export function parentRemotePath(path: string): string {
  const index = path.lastIndexOf('/');
  return index <= 0 ? '/' : path.slice(0, index);
}

// 本地文件路径取文件名：macOS/Linux 用 '/'，Windows 用 '\'（含 UNC 路径），两者都要切开。
// 盘符根路径（如 C:\）不是合法文件名，返回 undefined。
export function localFileName(localPath: string): string | undefined {
  const name = localPath.split(/[\\/]/).filter(Boolean).pop();
  if (!name || /^[A-Za-z]:$/.test(name)) return undefined;
  return name;
}

export function contextMenuPosition(
  x: number,
  y: number,
  menuWidth: number,
  menuHeight: number,
  viewportWidth: number,
  viewportHeight: number,
): { x: number; y: number } {
  const margin = 8;
  return {
    x: Math.max(margin, Math.min(x, viewportWidth - menuWidth - margin)),
    y: Math.max(margin, Math.min(y, viewportHeight - menuHeight - margin)),
  };
}

export function isDragPositionInside(
  position: { x: number; y: number },
  rect: { left: number; top: number; right: number; bottom: number },
): boolean {
  // WRY's macOS WKWebView bridge reports logical points even though the public
  // Tauri type is PhysicalPosition. Do not divide by devicePixelRatio again.
  return position.x >= rect.left && position.x <= rect.right && position.y >= rect.top && position.y <= rect.bottom;
}

// Tauri drag events are typed as PhysicalPosition. macOS WKWebView actually
// delivers logical points (see isDragPositionInside), but Windows WebView2
// reports true physical pixels, which must be scaled back to CSS coordinates
// before comparing against getBoundingClientRect().
export function normalizeDragPosition(
  position: { x: number; y: number },
  scaleFactor: number,
  reportsPhysicalPixels: boolean,
): { x: number; y: number } {
  if (!reportsPhysicalPixels || !Number.isFinite(scaleFactor) || scaleFactor <= 0 || scaleFactor === 1) {
    return position;
  }
  return { x: position.x / scaleFactor, y: position.y / scaleFactor };
}

export function platformReportsPhysicalDragPixels(userAgent: string): boolean {
  return /Windows/i.test(userAgent);
}

export function isSupportedArchive(name: string): boolean {
  return /\.(?:zip|tar|tgz|tar\.gz)$/i.test(name);
}

export function formatRemoteModified(modifiedAt: number | null): string {
  if (modifiedAt === null || !Number.isFinite(modifiedAt)) return '—';
  const date = new Date(modifiedAt * 1000);
  if (Number.isNaN(date.getTime())) return '—';
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function remoteArchiveConflictPath(reason: unknown): string | null {
  const message = reason instanceof Error ? reason.message : String(reason);
  if (message === '远程目标已存在，未覆盖') return '';
  const prefix = '解压目标已存在：';
  return message.startsWith(prefix) ? message.slice(prefix.length).trim() : null;
}

export function isRemoteArchiveConflict(reason: unknown): boolean {
  return remoteArchiveConflictPath(reason) !== null;
}

export function remoteFileError(reason: unknown, t: (key: MessageKey) => string): string {
  const message = reason instanceof Error ? reason.message : String(reason);
  if (message === '远程文本文件不能超过 1 MB') return t('remoteTextTooLarge');
  if (message === '远程文件包含二进制内容，不能作为文本编辑') return t('remoteBinaryFile');
  if (message === '远程文件不是有效的 UTF-8 文本') return t('remoteTextInvalidUtf8');
  if (message === '远程目标已存在，未覆盖') return t('transferAlreadyExists');
  if (message === '服务器缺少解压工具，请先安装 unzip 或 tar') return t('archiveToolMissing');
  if (message === '压缩包无法读取或已损坏') return t('archiveInvalid');
  if (message === '压缩包包含绝对路径或上级目录路径，已取消解压') return t('archiveUnsafe');
  if (message === '不支持这种压缩格式') return t('archiveUnsupported');
  if (message.startsWith('解压失败：')) return `${t('archiveFailed')}${message.slice('解压失败：'.length)}`;
  return message;
}
