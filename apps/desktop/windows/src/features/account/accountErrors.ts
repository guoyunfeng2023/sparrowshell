import type { Locale } from '../../i18n';

export function localizeAccountError(reason: unknown, locale: Locale): string {
  const message = reason instanceof Error ? reason.message : String(reason);
  const foreignKey = message.includes('FOREIGN KEY constraint failed');
  const localDatabase = message.startsWith('local database error:');
  const invalidSync = message.includes('invalid or inconsistent synchronization payload')
    || message.includes('cloud synchronization returned an invalid or incomplete response');

  if (locale === 'zh') {
    if (foreignKey) return '本地数据恢复失败：云端配置中的文件夹或跳板机引用不完整。';
    if (localDatabase) return '本地数据库操作失败，请稍后重试。';
    if (invalidSync) return '云端同步数据无效或不完整，请稍后重试。';
  } else {
    if (foreignKey) return 'Local restore failed: the cloud configuration contains an incomplete folder or jump-server reference.';
    if (localDatabase) return 'The local database operation failed. Please try again.';
    if (invalidSync) return 'The cloud synchronization data is invalid or incomplete. Please try again.';
  }
  return message;
}
