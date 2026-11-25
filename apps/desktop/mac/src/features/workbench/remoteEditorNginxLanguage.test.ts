import { EditorState } from '@codemirror/state';
import { ensureSyntaxTree } from '@codemirror/language';
import { describe, expect, it } from 'vitest';
import { nginxLanguage } from './remoteEditorNginxLanguage';

describe('Nginx syntax highlighting', () => {
  it('distinguishes blocks, directives, comments, values, variables and punctuation', () => {
    const source = [
      '# Main server',
      'upstream backend {',
      '  server 127.0.0.1:8080;',
      '}',
      'server {',
      '  listen 80;',
      '  server_name example.test;',
      '  location ~ \\.php$ {',
      '    proxy_pass http://$backend;',
      '    proxy_read_timeout 60s;',
      '  }',
      '}',
    ].join('\n');
    const state = EditorState.create({ doc: source, extensions: [nginxLanguage] });
    const tree = ensureSyntaxTree(state, source.length, 1000);
    const nodes = tree?.toString() ?? '';
    for (const token of ['comment', 'controlKeyword', 'keyword', 'propertyName', 'number', 'string', 'variableName', 'operator', 'punctuation']) {
      expect(nodes).toContain(token);
    }
    expect(state.doc.toString()).toBe(source);
  });

  it('keeps disabled directives as comments', () => {
    const source = '# server_name disabled.example.test;';
    const state = EditorState.create({ doc: source, extensions: [nginxLanguage] });
    const tree = ensureSyntaxTree(state, source.length, 1000);
    expect(tree?.toString()).toContain('comment');
    expect(tree?.toString()).not.toContain('propertyName');
  });
});
