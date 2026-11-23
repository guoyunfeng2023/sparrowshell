import { EditorState } from '@codemirror/state';
import { SearchQuery } from '@codemirror/search';
import { describe, expect, it } from 'vitest';
import { remoteEditorSearchExtensions } from './remoteEditorSearch';

describe('remote editor search and replace', () => {
  it('localizes the search and replace panel in Chinese', () => {
    const state = EditorState.create({ extensions: remoteEditorSearchExtensions('zh') });
    expect(state.phrase('Find')).toBe('查找');
    expect(state.phrase('Replace')).toBe('替换为');
    expect(state.phrase('replace all')).toBe('全部替换');
    expect(state.phrase('match case')).toBe('区分大小写');
    expect(state.phrase('by word')).toBe('全词匹配');
    expect(state.phrase('regexp')).toBe('正则表达式');
  });

  it('keeps the built-in English wording in English mode', () => {
    const state = EditorState.create({ extensions: remoteEditorSearchExtensions('en') });
    expect(state.phrase('Find')).toBe('Find');
    expect(state.phrase('Replace')).toBe('Replace');
    expect(state.phrase('replace all')).toBe('replace all');
  });

  it('supports case, whole-word and regular-expression query options', () => {
    const query = new SearchQuery({ search: 'server_[0-9]+', replace: 'host', caseSensitive: true, wholeWord: true, regexp: true });
    const state = EditorState.create({ doc: 'server_12\nServer_12\nserver_1234' });
    const cursor = query.getCursor(state);
    const first = cursor.next().value;
    const second = cursor.next().value;
    expect(state.doc.sliceString(first.from, first.to)).toBe('server_12');
    expect(state.doc.sliceString(second.from, second.to)).toBe('server_1234');
    expect(cursor.next().done).toBe(true);
  });
});
