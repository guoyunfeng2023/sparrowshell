export interface CompletionGeometry {
  containerWidth: number;
  containerHeight: number;
  cursorLeft: number;
  cursorTop: number;
  cursorWidth: number;
  cursorHeight: number;
  popupWidth: number;
  popupHeight: number;
}

export interface CompletionPosition {
  left: number;
  top: number;
  maxHeight: number;
}

const EDGE_GAP = 8;
const CURSOR_GAP = 4;

/** Place an IDE-like popup beside the cursor, flipping and clamping at edges. */
export function getCompletionPosition(geometry: CompletionGeometry): CompletionPosition {
  const { containerWidth, containerHeight, cursorLeft, cursorTop, cursorWidth, cursorHeight } = geometry;
  const popupWidth = Math.min(geometry.popupWidth, Math.max(0, containerWidth - EDGE_GAP * 2));
  const popupHeight = Math.min(geometry.popupHeight, Math.max(0, containerHeight - EDGE_GAP * 2));
  const left = Math.max(EDGE_GAP, Math.min(cursorLeft + cursorWidth, containerWidth - popupWidth - EDGE_GAP));

  const belowTop = cursorTop + cursorHeight + CURSOR_GAP;
  const belowSpace = Math.max(0, containerHeight - EDGE_GAP - belowTop);
  const aboveSpace = Math.max(0, cursorTop - CURSOR_GAP - EDGE_GAP);
  const showBelow = belowSpace >= popupHeight || belowSpace >= aboveSpace;
  const maxHeight = Math.max(0, showBelow ? belowSpace : aboveSpace);
  const visibleHeight = Math.min(popupHeight, maxHeight);
  const top = showBelow ? belowTop : Math.max(EDGE_GAP, cursorTop - CURSOR_GAP - visibleHeight);

  return {
    left: Math.round(left * 100) / 100,
    top: Math.round(top * 100) / 100,
    maxHeight: Math.floor(maxHeight),
  };
}
