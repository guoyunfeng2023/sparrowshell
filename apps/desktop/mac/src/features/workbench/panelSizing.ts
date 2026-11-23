export const MIN_AI_PANEL_HEIGHT = 180;
export const MIN_WORKBENCH_HEIGHT = 260;

export function clampAiPanelHeight(height: number, containerHeight: number): number {
  const maximum = Math.max(MIN_AI_PANEL_HEIGHT, containerHeight - MIN_WORKBENCH_HEIGHT);
  return Math.min(maximum, Math.max(MIN_AI_PANEL_HEIGHT, height));
}

export function draggedAiPanelHeight(
  startHeight: number,
  startY: number,
  pointerY: number,
  containerHeight: number,
): number {
  return clampAiPanelHeight(startHeight + startY - pointerY, containerHeight);
}
