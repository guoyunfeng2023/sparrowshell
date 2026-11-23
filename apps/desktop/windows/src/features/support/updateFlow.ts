import type { UpdateCheck, UpdateDownloadProgress } from '../../types';

export function shouldShowUpdatePrompt(update: UpdateCheck, manual: boolean): boolean {
  return update.status === 'update_available' && (manual || update.updatePolicy !== 'silent');
}

export function canDismissUpdatePrompt(update: UpdateCheck, manual: boolean): boolean {
  return manual || update.updatePolicy !== 'force';
}

export function updateProgressPercent(progress: UpdateDownloadProgress | null): number | null {
  if (!progress || !progress.total || progress.total <= 0) return null;
  return Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
}
