import { describe, expect, it } from 'vitest';
import { messages } from './i18n';

 describe('desktop i18n contract', () => {
  it('provides every message in Chinese and English', () => {
    expect(Object.keys(messages.zh).sort()).toEqual(Object.keys(messages.en).sort());
  });

  it('keeps security and real-connection boundaries explicit', () => {
    expect(messages.zh.secret).toContain('永不同步');
    expect(messages.en.secret).toContain('never synced');
    expect(messages.zh.aiLater).toContain('不提供模拟');
    expect(messages.en.preview).toContain('real storage and SSH');
    expect(messages.zh.confirmOverwrite).toContain('独立确认');
    expect(messages.en.confirmOverwrite).toContain('separate confirmation');
  });

  it('localizes expired command approval', () => {
    expect(messages.zh.approvalExpired).toBe('命令执行授权已过期，请重新执行任务并及时确认。');
    expect(messages.en.approvalExpired).toBe('Command approval has expired. Run the task again and confirm promptly.');
  });

  it('explains completion sources and both Enter behaviors', () => {
    expect(messages.zh.completionSources).toContain('本机历史');
    expect(messages.zh.completionSources).toContain('Tab');
    expect(messages.zh.completionHint).toContain('执行当前输入');
    expect(messages.zh.completionHintSelected).toContain('再按 Enter 执行');
    expect(messages.en.completionSources).toContain('native Shell completion');
    expect(messages.en.completionHint).toContain('runs input');
    expect(messages.en.completionHintSelected).toContain('Press Enter again to run');
  });
});
