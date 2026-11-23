import { describe, expect, it } from 'vitest';
import type { CatalogSnapshot, Folder, ServerRecord } from '../../types';
import { folderOrderChanges, serverOrderChanges } from './catalogOrdering';

function folder(id: string, parentId: string | null = null, sortOrder = 0): Folder {
  return { id, parentId, name: id, sortOrder, revision: 1, updatedAt: '2026-10-31T00:00:00Z' };
}

function server(id: string, folderId: string | null = null, sortOrder = 0): ServerRecord {
  return {
    id, folderId, name: id, host: `${id}.example.com`, port: 22, username: 'root', authType: 'password',
    privateKeyPathHint: null, jumpServerId: null, sortOrder, revision: 1,
    updatedAt: '2026-10-31T00:00:00Z', deletedAt: null,
  };
}

function catalog(folders: Folder[], servers: ServerRecord[]): CatalogSnapshot {
  return { folders, servers };
}

describe('catalog ordering', () => {
  it('does not reorder servers within their current folder', () => {
    const changes = serverOrderChanges(catalog([], [server('a', 'f', 0), server('b', 'f', 1), server('c', 'f', 2)]), 'a', {
      parentId: 'f', targetId: 'b', position: 'after',
    });
    expect(changes).toEqual([]);
  });

  it('moves only the dragged server without changing any order values', () => {
    const changes = serverOrderChanges(catalog([], [server('a', 'f1', 0), server('b', 'f2', 0), server('c', 'f2', 1)]), 'c', {
      parentId: 'f1', position: 'inside',
    });
    expect(changes).toEqual([{ id: 'c', parentId: 'f1', sortOrder: 1 }]);
  });

  it('reorders folders without changing their parent', () => {
    const changes = folderOrderChanges(catalog([folder('a', null, 0), folder('b', null, 1), folder('c', null, 2)], []), 'c', {
      parentId: null, targetId: 'a', position: 'before',
    });
    expect(changes?.map((item) => item.id)).toEqual(['c', 'a', 'b']);
  });

  it('rejects moving a folder into itself or a descendant', () => {
    const value = catalog([folder('a'), folder('b', 'a'), folder('c', 'b')], []);
    expect(folderOrderChanges(value, 'a', { parentId: 'a', position: 'inside' })).toBeNull();
    expect(folderOrderChanges(value, 'a', { parentId: 'c', position: 'inside' })).toBeNull();
  });
});
