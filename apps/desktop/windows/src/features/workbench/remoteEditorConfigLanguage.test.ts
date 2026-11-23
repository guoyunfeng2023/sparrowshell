import { EditorState } from '@codemirror/state';
import { ensureSyntaxTree } from '@codemirror/language';
import { describe, expect, it } from 'vitest';
import { dotenvLanguage } from './remoteEditorConfigLanguage';

describe('dotenv syntax highlighting', () => {
  it('distinguishes comments, keys, values, numbers, booleans and variable references', () => {
    const source = [
      '# Server settings',
      'export API_URL="https://example.test"',
      'PORT=8080',
      'DEBUG=true',
      'BASE_URL=${API_URL}',
    ].join('\n');
    const state = EditorState.create({ doc: source, extensions: [dotenvLanguage] });
    const tree = ensureSyntaxTree(state, source.length, 1000);
    const nodes = tree?.toString() ?? '';
    for (const token of ['comment', 'keyword', 'propertyName', 'string', 'number', 'bool', 'variableName']) {
      expect(nodes).toContain(token);
    }
  });

  it('does not modify environment values', () => {
    const source = 'PASSWORD=p@ss#word';
    const state = EditorState.create({ doc: source, extensions: [dotenvLanguage] });
    ensureSyntaxTree(state, source.length, 1000);
    expect(state.doc.toString()).toBe(source);
  });
});
