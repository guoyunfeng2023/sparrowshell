import { describe, expect, it } from 'vitest';
import { shouldDismissCompletion } from './completionDismissal';

function region(children: object[]) {
  return { contains: (target: object) => children.includes(target) };
}

describe('completion dismissal', () => {
  it('dismisses on outside pointer interactions but not inside the popup', () => {
    const outside = {};
    const option = {};
    const popup = region([option]);

    expect(shouldDismissCompletion(outside, [popup])).toBe(true);
    expect(shouldDismissCompletion(option, [popup])).toBe(false);
    expect(shouldDismissCompletion(null, [popup])).toBe(true);
  });

  it('dismisses for terminal output and workspace scrolling, but not popup scrolling', () => {
    const workspace = {};
    const terminalOutput = {};
    const completionList = {};
    const popup = region([completionList]);

    expect(shouldDismissCompletion(workspace, [popup])).toBe(true);
    expect(shouldDismissCompletion(terminalOutput, [popup])).toBe(true);
    expect(shouldDismissCompletion(completionList, [popup])).toBe(false);
  });
});
