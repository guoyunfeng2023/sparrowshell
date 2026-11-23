import { describe, expect, it } from 'vitest';
import { CommandLineRecorder } from './commandRecorder';

describe('CommandLineRecorder', () => {
  it('captures unicode commands and local backspace editing', () => {
    const recorder = new CommandLineRecorder();
    expect(recorder.accept('printf 你好x\u007f\r')).toEqual(['printf 你好']);
  });

  it('supports cursor movement and insertion without executing anything', () => {
    const recorder = new CommandLineRecorder();
    recorder.accept('echo ac\u001b[Db');
    expect(recorder.accept('\r')).toEqual(['echo abc']);
  });

  it('drops uncertain shell completion and recalled history', () => {
    const recorder = new CommandLineRecorder();
    recorder.accept('ec\t');
    expect(recorder.accept('\r')).toEqual([]);
    recorder.accept('ls\u001b[A');
    expect(recorder.accept('\r')).toEqual([]);
  });

  it('exposes a confident line snapshot for suffix-only completion', () => {
    const recorder = new CommandLineRecorder();
    recorder.accept('systemctl st');
    expect(recorder.snapshot()).toEqual({ text: 'systemctl st', cursor: 12, confident: true });
    recorder.accept('\u001b[D');
    expect(recorder.snapshot()).toEqual({ text: 'systemctl st', cursor: 11, confident: true });
    recorder.clear();
    expect(recorder.snapshot()).toEqual({ text: '', cursor: 0, confident: true });
  });

  it('marks native completion and recalled history as unsafe for smart completion', () => {
    const recorder = new CommandLineRecorder();
    recorder.accept('docker p\t');
    expect(recorder.snapshot().confident).toBe(false);
    recorder.clear();
    recorder.accept('docker p\u001b[A');
    expect(recorder.snapshot().confident).toBe(false);
  });

  it('supports multiline paste and Ctrl+C reset', () => {
    const recorder = new CommandLineRecorder();
    expect(recorder.accept('pwd\r\nuname -a\r')).toEqual(['pwd', 'uname -a']);
    recorder.accept('secret-ish partial\u0003');
    expect(recorder.accept('df -h\r')).toEqual(['df -h']);
  });
});
