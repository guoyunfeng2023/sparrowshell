import { StreamLanguage } from '@codemirror/language';
import { tags } from '@lezer/highlight';

interface NginxState {
  expectDirective: boolean;
}

const blockDirectives = new Set([
  'events', 'geo', 'http', 'if', 'limit_except', 'location', 'mail', 'map',
  'server', 'stream', 'types', 'upstream',
]);

const importantDirectives = new Set([
  'fastcgi_pass', 'include', 'internal', 'listen', 'proxy_pass', 'root',
  'server_name', 'try_files',
]);

/**
 * Highlights Nginx configuration without evaluating or changing its content.
 * Directives are recognized at the beginning of a directive, while values,
 * variables, comments and numbers receive independent highlight tags.
 */
export const nginxLanguage = StreamLanguage.define<NginxState>({
  name: 'nginx',
  startState: () => ({ expectDirective: true }),
  token(stream, state) {
    if (stream.sol()) state.expectDirective = true;
    if (stream.eatSpace()) return null;
    if (stream.match(/^#.*/)) return 'comment';

    if (stream.match(/^[{};]/)) {
      state.expectDirective = true;
      return 'punctuation';
    }

    if (stream.match(/^\$\{?[A-Za-z_][A-Za-z0-9_]*\}?/)) return 'variableName';
    if (stream.match(/^https?:\/\/[^\s;{}#$]+/i)) return 'string';
    if (stream.match(/^"(?:\\.|[^"\\])*"/) || stream.match(/^'(?:\\.|[^'\\])*'/)) return 'string';
    if (stream.match(/^"[^"\n]*$/) || stream.match(/^'[^'\n]*$/)) return 'string';
    if (stream.match(/^[0-9]+(?:\.[0-9]+)*(?::[0-9]+)?(?:ms|s|m|h|d|k|g|%)?/i)) return 'number';
    if (stream.match(/^\/[^\s;{}#$]+/)) return 'string';
    if (stream.match(/^(?:\^~|=|~\*?|!~\*?)/)) return 'operator';

    if (stream.match(/^[A-Za-z_][A-Za-z0-9_-]*/)) {
      const word = stream.current();
      if (state.expectDirective) {
        state.expectDirective = false;
        if (blockDirectives.has(word)) return 'controlKeyword';
        if (importantDirectives.has(word)) return 'keyword';
        return 'propertyName';
      }
      if (/^(?:on|off|true|false)$/i.test(word)) return 'bool';
      return 'string';
    }

    stream.next();
    return null;
  },
  tokenTable: {
    comment: tags.comment,
    controlKeyword: tags.controlKeyword,
    keyword: tags.keyword,
    propertyName: tags.propertyName,
    variableName: tags.variableName,
    string: tags.string,
    number: tags.number,
    bool: tags.bool,
    operator: tags.operator,
    punctuation: tags.punctuation,
  },
});
