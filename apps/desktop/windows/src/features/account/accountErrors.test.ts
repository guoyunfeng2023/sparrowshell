import { describe, expect, it } from 'vitest';
import { localizeAccountError } from './accountErrors';

describe('localizeAccountError', () => {
  it('localizes foreign-key and database failures in Chinese', () => {
    expect(localizeAccountError('local database error: FOREIGN KEY constraint failed', 'zh'))
      .toBe('本地数据恢复失败：云端配置中的文件夹或跳板机引用不完整。');
    expect(localizeAccountError('local database error: database is locked', 'zh'))
      .toBe('本地数据库操作失败，请稍后重试。');
  });

  it('localizes known failures in English and preserves unknown messages', () => {
    expect(localizeAccountError('local database error: FOREIGN KEY constraint failed', 'en'))
      .toContain('Local restore failed');
    expect(localizeAccountError('custom account error', 'zh')).toBe('custom account error');
  });
});
