import type { CatalogOrderItem, CatalogSnapshot } from '../../types';

export type CatalogDropPosition = 'before' | 'after' | 'inside' | 'end';

export interface CatalogDropTarget {
  parentId: string | null;
  targetId?: string;
  position: CatalogDropPosition;
}

interface Orderable {
  id: string;
}

function reorder<T extends Orderable>(
  all: T[],
  draggedId: string,
  destination: CatalogDropTarget,
  parentOf: (item: T) => string | null,
): CatalogOrderItem[] {
  const dragged = all.find((item) => item.id === draggedId);
  if (!dragged) return [];
  const sourceParent = parentOf(dragged);
  const siblings = all.filter((item) => parentOf(item) === destination.parentId && item.id !== draggedId);
  let insertAt = siblings.length;
  if (destination.targetId) {
    const targetIndex = siblings.findIndex((item) => item.id === destination.targetId);
    if (targetIndex < 0) return [];
    insertAt = targetIndex + (destination.position === 'after' ? 1 : 0);
  }
  siblings.splice(insertAt, 0, dragged);
  const changes = siblings.map((item, sortOrder) => ({ id: item.id, parentId: destination.parentId, sortOrder }));
  if (sourceParent !== destination.parentId) {
    changes.push(...all
      .filter((item) => parentOf(item) === sourceParent && item.id !== draggedId)
      .map((item, sortOrder) => ({ id: item.id, parentId: sourceParent, sortOrder })));
  }
  return changes;
}

export function serverOrderChanges(catalog: CatalogSnapshot, draggedId: string, destination: CatalogDropTarget): CatalogOrderItem[] {
  const dragged = catalog.servers.find((server) => server.id === draggedId);
  if (!dragged || dragged.folderId === destination.parentId) return [];
  return [{ id: dragged.id, parentId: destination.parentId, sortOrder: dragged.sortOrder }];
}

export function folderOrderChanges(catalog: CatalogSnapshot, draggedId: string, destination: CatalogDropTarget): CatalogOrderItem[] | null {
  const dragged = catalog.folders.find((folder) => folder.id === draggedId);
  if (!dragged || destination.position === 'inside' && dragged.parentId === destination.parentId) return [];
  let ancestor = destination.parentId;
  while (ancestor) {
    if (ancestor === draggedId) return null;
    ancestor = catalog.folders.find((folder) => folder.id === ancestor)?.parentId ?? null;
  }
  return reorder(catalog.folders, draggedId, destination, (folder) => folder.parentId);
}
