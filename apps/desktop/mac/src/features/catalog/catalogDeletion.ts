import type { CatalogSnapshot } from '../../types';

export interface FolderDeletionImpact {
  folderIds: Set<string>;
  serverIds: Set<string>;
  subfolderCount: number;
  serverCount: number;
}

export function folderDeletionImpact(catalog: Pick<CatalogSnapshot, 'folders' | 'servers'>, folderId: string): FolderDeletionImpact {
  const folderIds = new Set([folderId]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const folder of catalog.folders) {
      if (folder.parentId && folderIds.has(folder.parentId) && !folderIds.has(folder.id)) {
        folderIds.add(folder.id);
        changed = true;
      }
    }
  }
  const serverIds = new Set(catalog.servers.filter((server) => server.folderId && folderIds.has(server.folderId)).map((server) => server.id));
  return { folderIds, serverIds, subfolderCount: folderIds.size - 1, serverCount: serverIds.size };
}
