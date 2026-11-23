const modifierOnlyKeys = new Set(['Alt', 'Control', 'Meta', 'Shift']);

/** Let native modifiers and macOS copy bypass xterm's user-input scrolling. */
export function shouldBypassTerminalKeyEvent(event: Pick<KeyboardEvent, 'key' | 'metaKey'>): boolean {
  return modifierOnlyKeys.has(event.key) || (event.metaKey && event.key.toLowerCase() === 'c');
}

export type CompletionKeyAction = 'open' | 'close' | 'previous' | 'next' | 'accept' | 'ignore' | null;

export type CompletionPointerPosition = { x: number; y: number };

/** Ignore mousemove events emitted when scrollIntoView moves options under a stationary pointer. */
export function shouldSelectCompletionOnPointerMove(
  previous: CompletionPointerPosition | null,
  next: CompletionPointerPosition,
): boolean {
  return previous !== null && (previous.x !== next.x || previous.y !== next.y);
}

export function getCompletionSelectionIndex(current: number, count: number, direction: 'previous' | 'next'): number {
  if (count <= 0) return 0;
  return direction === 'previous' ? (current - 1 + count) % count : (current + 1) % count;
}

/** Keep Return and Tab owned by the remote shell, even while suggestions are visible. */
export function terminalCompletionKeyAction(
  event: Pick<KeyboardEvent, 'type' | 'key' | 'code' | 'ctrlKey' | 'metaKey' | 'altKey'>,
  open: boolean,
  selectionActive = false,
  enabled = true,
): CompletionKeyAction {
  if (!enabled) return null;
  const opensCompletion = event.ctrlKey && !event.metaKey && !event.altKey && event.code === 'Space';
  const controlsCompletion = open && (['Escape', 'ArrowUp', 'ArrowDown', 'ArrowRight'].includes(event.key) || (selectionActive && event.key === 'Enter'));
  // Xterm sends its custom handler for multiple event phases. Consume follow-up
  // events without repeating actions. Enter is always consumed after keydown so
  // accepting a candidate cannot leak a keypress and execute the filled command.
  if (event.type !== 'keydown') return opensCompletion || controlsCompletion || event.key === 'Enter' ? 'ignore' : null;
  if (opensCompletion) return 'open';
  if (!open) return null;
  if (event.key === 'Escape') return 'close';
  if (event.key === 'ArrowUp') return 'previous';
  if (event.key === 'ArrowDown') return 'next';
  if (event.key === 'ArrowRight' || (selectionActive && event.key === 'Enter')) return 'accept';
  return null;
}
