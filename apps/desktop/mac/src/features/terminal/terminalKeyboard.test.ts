import { describe, expect, it } from 'vitest';
import { getCompletionSelectionIndex, shouldBypassTerminalKeyEvent, shouldSelectCompletionOnPointerMove, terminalCompletionKeyAction } from './terminalKeyboard';

describe('terminal keyboard scrolling', () => {
  it.each(['Meta', 'Shift', 'Control', 'Alt'])('does not treat the %s modifier as terminal input', (key) => {
    expect(shouldBypassTerminalKeyEvent({ key, metaKey: key === 'Meta' })).toBe(true);
  });

  it('lets native Command+C copy without moving the viewport', () => {
    expect(shouldBypassTerminalKeyEvent({ key: 'c', metaKey: true })).toBe(true);
    expect(shouldBypassTerminalKeyEvent({ key: 'C', metaKey: true })).toBe(true);
  });

  it('keeps normal terminal input and Ctrl+C unchanged', () => {
    expect(shouldBypassTerminalKeyEvent({ key: 'c', metaKey: false })).toBe(false);
    expect(shouldBypassTerminalKeyEvent({ key: 'Enter', metaKey: false })).toBe(false);
    expect(shouldBypassTerminalKeyEvent({ key: 'v', metaKey: true })).toBe(false);
  });
});

describe('terminal completion keyboard', () => {
  const key = (value: string, code = value, type = 'keydown') => ({ type, key: value, code, ctrlKey: false, metaKey: false, altKey: false });

  it('opens only with unmodified Ctrl+Space', () => {
    expect(terminalCompletionKeyAction({ ...key(' ', 'Space'), ctrlKey: true }, false)).toBe('open');
    expect(terminalCompletionKeyAction({ ...key(' ', 'Space'), ctrlKey: true, metaKey: true }, false)).toBeNull();
    expect(terminalCompletionKeyAction(key(' ', 'Space'), false)).toBeNull();
  });

  it('leaves keyboard input untouched when completion is disabled', () => {
    expect(terminalCompletionKeyAction({ ...key(' ', 'Space'), ctrlKey: true }, false, false, false)).toBeNull();
    expect(terminalCompletionKeyAction(key('ArrowDown'), false, false, false)).toBeNull();
    expect(terminalCompletionKeyAction(key('Enter'), false, false, false)).toBeNull();
  });

  it('handles selection only while the completion popup is open', () => {
    expect(terminalCompletionKeyAction(key('ArrowUp'), true)).toBe('previous');
    expect(terminalCompletionKeyAction(key('ArrowDown'), true)).toBe('next');
    expect(terminalCompletionKeyAction(key('ArrowRight'), true)).toBe('accept');
    expect(terminalCompletionKeyAction(key('Escape'), true)).toBe('close');
    expect(terminalCompletionKeyAction(key('ArrowUp'), false)).toBeNull();
  });

  it('uses Enter to fill only after the user actively selects a candidate', () => {
    expect(terminalCompletionKeyAction(key('Enter'), true, false)).toBeNull();
    expect(terminalCompletionKeyAction(key('Enter'), true, true)).toBe('accept');
    expect(terminalCompletionKeyAction(key('Enter', 'Enter', 'keyup'), true, true)).toBe('ignore');
    expect(terminalCompletionKeyAction(key('Enter', 'Enter', 'keypress'), false, false)).toBe('ignore');
    expect(terminalCompletionKeyAction(key('ArrowRight'), true, false)).toBe('accept');
  });

  it('moves only once for a physical arrow key down/up cycle', () => {
    let selected = 0;
    for (const event of [key('ArrowDown', 'ArrowDown', 'keydown'), key('ArrowDown', 'ArrowDown', 'keyup')]) {
      if (terminalCompletionKeyAction(event, true) === 'next') selected += 1;
    }
    expect(selected).toBe(1);
    expect(terminalCompletionKeyAction(key('ArrowUp', 'ArrowUp', 'keyup'), true)).toBe('ignore');
    expect(terminalCompletionKeyAction(key('ArrowRight', 'ArrowRight', 'keyup'), true)).toBe('ignore');
  });

  it('wraps from the first item to the last and from the last to the first', () => {
    expect(getCompletionSelectionIndex(0, 8, 'previous')).toBe(7);
    expect(getCompletionSelectionIndex(7, 8, 'next')).toBe(0);
    expect(getCompletionSelectionIndex(0, 1, 'previous')).toBe(0);
    expect(getCompletionSelectionIndex(0, 0, 'next')).toBe(0);
  });

  it('keeps keyboard selection stable when a multi-page list scrolls under a stationary pointer', () => {
    const pointer = { x: 240, y: 180 };
    let selected = 0;
    for (let index = 0; index < 12; index += 1) {
      selected = getCompletionSelectionIndex(selected, 30, 'next');
    }
    expect(selected).toBe(12);

    // scrollIntoView can emit mousemove for option 5 at the unchanged pointer position.
    if (shouldSelectCompletionOnPointerMove(pointer, { x: 240, y: 180 })) selected = 5;
    expect(selected).toBe(12);

    // A genuine pointer movement can still take over selection.
    expect(shouldSelectCompletionOnPointerMove(pointer, { x: 240, y: 196 })).toBe(true);
    expect(shouldSelectCompletionOnPointerMove(null, { x: 240, y: 180 })).toBe(false);
  });

  it('never takes Return or native Tab away from the shell', () => {
    expect(terminalCompletionKeyAction(key('Enter'), true)).toBeNull();
    expect(terminalCompletionKeyAction(key('Tab'), true)).toBeNull();
  });
});
