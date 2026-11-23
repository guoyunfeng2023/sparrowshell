import { describe, expect, it } from 'vitest';
import type { CatalogSnapshot, Folder, ServerRecord } from '../../types';
import { folderDeletionImpact } from './catalogDeletion';

const folder = (id: string, parentId: string | null = null): Folder => ({ id, parentId, name: id, sortOrder: 0, revision: 1, updatedAt: '2026-01-01T00:00:00Z' });
const server = (id: string, folderId: string | null): ServerRecord => ({
  id, folderId, name: id, host: `${id}.example.com`, port: 22, username: 'root', authType: 'password', privateKeyPathHint: null, jumpServerId: null, sortOrder: 0, revision: 1, updatedAt: '2026-01-01T00:00:00Z', deletedAt: null,
});
const catalog = (folders: Folder[], servers: ServerRecord[]): CatalogSnapshot => ({ folders, servers });

describe('folder deletion impact', () => {
  it('allows an empty folder to be deleted without confirmation', () => {
    const impact = folderDeletionImpact(catalog([folder('empty')], []), 'empty');
    expect(impact.serverCount).toBe(0);
    expect(impact.subfolderCount).toBe(0);
  });

  it('does not require confirmation for nested folders without servers', () => {
    const impact = folderDeletionImpact(catalog([folder('parent'), folder('child', 'parent'), folder('grandchild', 'child')], []), 'parent');
    expect(impact.serverCount).toBe(0);
    expect(impact.subfolderCount).toBe(2);
  });

  it('requires confirmation when the folder directly contains servers', () => {
    const impact = folderDeletionImpact(catalog([folder('parent')], [server('server-a', 'parent')]), 'parent');
    expect(impact.serverCount).toBe(1);
    expect([...impact.serverIds]).toEqual(['server-a']);
  });

  it('requires confirmation when only a descendant folder contains servers', () => {
    const impact = folderDeletionImpact(catalog(
      [folder('parent'), folder('child', 'parent'), folder('grandchild', 'child'), folder('unrelated')],
      [server('nested-server', 'grandchild'), server('outside-server', 'unrelated'), server('root-server', null)],
    ), 'parent');
    expect(impact.serverCount).toBe(1);
    expect([...impact.serverIds]).toEqual(['nested-server']);
    expect(impact.subfolderCount).toBe(2);
  });
});
