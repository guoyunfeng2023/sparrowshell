import { describe, expect, it } from 'vitest';
import {
  clampAiPanelHeight,
  draggedAiPanelHeight,
  MIN_AI_PANEL_HEIGHT,
} from './panelSizing';

describe('workbench and AI panel sizing', () => {
  it('moves the divider in the same direction as the pointer', () => {
    expect(draggedAiPanelHeight(320, 500, 460, 900)).toBe(360);
    expect(draggedAiPanelHeight(320, 500, 540, 900)).toBe(280);
  });

  it('keeps both regions usable at the drag boundaries', () => {
    expect(clampAiPanelHeight(20, 900)).toBe(MIN_AI_PANEL_HEIGHT);
    expect(clampAiPanelHeight(900, 900)).toBe(640);
  });

  it('falls back to the AI minimum in an unusually short window', () => {
    expect(clampAiPanelHeight(500, 320)).toBe(MIN_AI_PANEL_HEIGHT);
  });
});
