import { EditorState } from '@codemirror/state';
import { search } from '@codemirror/search';
import type { Locale } from '../../i18n';

const chinesePhrases = {
  Find: '查找',
  Replace: '替换为',
  next: '下一个',
  previous: '上一个',
  all: '选择全部匹配',
  'match case': '区分大小写',
  regexp: '正则表达式',
  'by word': '全词匹配',
  replace: '替换',
  'replace all': '全部替换',
  close: '关闭',
  'Go to line': '跳转到行号',
  go: '跳转',
  'current match': '当前匹配项',
  'on line': '所在行',
  'replaced match on line $': '已替换第 $ 行的匹配项',
  'replaced $ matches': '已替换 $ 处匹配项',
};

export function remoteEditorSearchExtensions(locale: Locale) {
  return [
    search({ top: true }),
    EditorState.phrases.of(locale === 'zh' ? chinesePhrases : {}),
  ];
}
