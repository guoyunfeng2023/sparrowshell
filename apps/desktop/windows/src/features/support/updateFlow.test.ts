import { describe, expect, it } from 'vitest';
import type { UpdateCheck } from '../../types';
import { canDismissUpdatePrompt, shouldShowUpdatePrompt, updateProgressPercent } from './updateFlow';

function update(updatePolicy: UpdateCheck['updatePolicy']): UpdateCheck {
  return {
    status: 'update_available', version: '1.1.0', minimumMacOs: '13.0.0', target: 'darwin-aarch64',
    channel: 'stable', updatePolicy, releaseNotes: 'Improvements', artifactUrl: 'https://example.com/app.dmg',
    artifactBytes: null, sha256: null, signature: null, publishedAt: null,
  };
}

describe('update prompt flow', () => {
  it('shows force and optional updates at startup', () => {
    expect(shouldShowUpdatePrompt(update('force'), false)).toBe(true);
    expect(shouldShowUpdatePrompt(update('optional'), false)).toBe(true);
  });

  it('suppresses silent updates only at startup', () => {
    expect(shouldShowUpdatePrompt(update('silent'), false)).toBe(false);
    expect(shouldShowUpdatePrompt(update('silent'), true)).toBe(true);
  });

  it('allows cancellation of every manual update prompt', () => {
    expect(canDismissUpdatePrompt(update('force'), true)).toBe(true);
    expect(canDismissUpdatePrompt(update('optional'), true)).toBe(true);
    expect(canDismissUpdatePrompt(update('silent'), true)).toBe(true);
  });

  it('accepts Windows update metadata without changing the prompt policy', () => {
    const windowsUpdate: UpdateCheck = { ...update('optional'), target: 'windows-x86_64', minimumMacOs: null };
    expect(windowsUpdate.target).toBe('windows-x86_64');
    expect(shouldShowUpdatePrompt(windowsUpdate, false)).toBe(true);
    expect(canDismissUpdatePrompt(windowsUpdate, false)).toBe(true);
  });

  it('blocks cancellation only for a forced startup update', () => {
    expect(canDismissUpdatePrompt(update('force'), false)).toBe(false);
    expect(canDismissUpdatePrompt(update('optional'), false)).toBe(true);
  });
});

describe('update download progress', () => {
  it('reports no percent without a known total', () => {
    expect(updateProgressPercent(null)).toBeNull();
    expect(updateProgressPercent({ downloaded: 100, total: null })).toBeNull();
    expect(updateProgressPercent({ downloaded: 100, total: 0 })).toBeNull();
  });

  it('rounds and clamps the percent', () => {
    expect(updateProgressPercent({ downloaded: 50, total: 200 })).toBe(25);
    expect(updateProgressPercent({ downloaded: 1, total: 3 })).toBe(33);
    expect(updateProgressPercent({ downloaded: 500, total: 200 })).toBe(100);
  });
});
