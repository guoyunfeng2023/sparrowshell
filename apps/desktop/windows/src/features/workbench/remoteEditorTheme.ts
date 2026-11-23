import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags } from '@lezer/highlight';

const lightHighlightStyle = HighlightStyle.define([
  { tag: tags.comment, color: '#3d7a46', fontStyle: 'italic' },
  { tag: [tags.propertyName, tags.definition(tags.variableName)], color: '#7950a0' },
  { tag: [tags.string, tags.special(tags.string)], color: '#a15c24' },
  { tag: [tags.keyword, tags.controlKeyword, tags.moduleKeyword], color: '#2166b1' },
  { tag: [tags.number, tags.bool, tags.null], color: '#087f8c' },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: '#174ea6' },
  { tag: [tags.variableName, tags.special(tags.variableName)], color: '#3568a8' },
  { tag: [tags.typeName, tags.className, tags.namespace], color: '#7950a0' },
  { tag: [tags.operator, tags.punctuation], color: '#58616c' },
  { tag: [tags.link, tags.url], color: '#2166b1', textDecoration: 'underline' },
  { tag: [tags.heading, tags.strong], color: '#253241', fontWeight: '600' },
  { tag: tags.emphasis, fontStyle: 'italic' },
  { tag: tags.strikethrough, textDecoration: 'line-through' },
  { tag: tags.invalid, color: '#c03535' },
]);

export const remoteEditorLightHighlighting = syntaxHighlighting(lightHighlightStyle);
