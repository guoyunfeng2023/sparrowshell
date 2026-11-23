import { StreamLanguage } from '@codemirror/language';
import { tags } from '@lezer/highlight';

interface DotenvState {
  inValue: boolean;
}

/**
 * A deliberately small dotenv parser. It only colors configuration syntax and
 * never evaluates values, expands variables, or changes the remote content.
 */
export const dotenvLanguage = StreamLanguage.define<DotenvState>({
  name: 'dotenv',
  startState: () => ({ inValue: false }),
  token(stream, state) {
    if (stream.sol()) {
      state.inValue = false;
      if (stream.match(/^\s*[#;]/)) {
        stream.skipToEnd();
        return 'comment';
      }
      if (stream.match(/^\s*export(?=\s+)/)) return 'keyword';
    }

    if (!state.inValue) {
      if (stream.match(/^\s*[=:]\s*/)) {
        state.inValue = true;
        return 'punctuation';
      }
      if (stream.eatSpace()) return null;
      stream.next();
      return 'propertyName';
    }

    if (stream.match(/^\s+[#;].*/)) return 'comment';
    if (stream.eatSpace()) return null;
    if (stream.match(/^"(?:\\.|[^"\\])*"/) || stream.match(/^'(?:\\.|[^'\\])*'/)) return 'string';
    if (stream.match(/^(?:true|false|null)(?=\s|$)/i)) return 'bool';
    if (stream.match(/^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?=\s|$)/)) return 'number';
    if (stream.match(/^\$\{?[A-Za-z_][A-Za-z0-9_]*\}?/)) return 'variableName';
    stream.next();
    return 'string';
  },
  tokenTable: {
    comment: tags.comment,
    keyword: tags.keyword,
    propertyName: tags.propertyName,
    punctuation: tags.punctuation,
    string: tags.string,
    bool: tags.bool,
    number: tags.number,
    variableName: tags.variableName,
  },
});
