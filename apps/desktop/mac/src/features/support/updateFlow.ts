import type { UpdateCheck } from '../../types';

export function shouldShowUpdatePrompt(update: UpdateCheck, manual: boolean): boolean {
  return update.status === 'update_available' && (manual || update.updatePolicy !== 'silent');
}

export function canDismissUpdatePrompt(update: UpdateCheck, manual: boolean): boolean {
  return manual || update.updatePolicy !== 'force';
}
