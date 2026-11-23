import { describe, expect, it } from 'vitest';
import { getCompletionPosition } from './completionPosition';

const base = {
  containerWidth: 800,
  containerHeight: 600,
  cursorLeft: 100,
  cursorTop: 100,
  cursorWidth: 8,
  cursorHeight: 18,
  popupWidth: 460,
  popupHeight: 300,
};

describe('completion cursor positioning', () => {
  it('places the popup immediately after and below the cursor', () => {
    expect(getCompletionPosition(base)).toEqual({ left: 108, top: 122, maxHeight: 470 });
  });

  it('clamps horizontally when the command is near the right edge', () => {
    const position = getCompletionPosition({ ...base, cursorLeft: 760 });
    expect(position.left).toBe(332);
    expect(position.top).toBe(122);
  });

  it('flips above the cursor when there is more space above', () => {
    const position = getCompletionPosition({ ...base, cursorTop: 500 });
    expect(position).toEqual({ left: 108, top: 196, maxHeight: 488 });
  });

  it('uses the larger available side and constrains height in a small terminal', () => {
    const position = getCompletionPosition({ ...base, containerHeight: 300, cursorTop: 50 });
    expect(position).toEqual({ left: 108, top: 72, maxHeight: 220 });
  });

  it('keeps the popup inside narrow containers', () => {
    const position = getCompletionPosition({ ...base, containerWidth: 320, cursorLeft: 300 });
    expect(position.left).toBe(8);
    expect(position.left).toBeGreaterThanOrEqual(8);
  });
});
