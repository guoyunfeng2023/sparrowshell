import { LanguageDescription, LanguageSupport } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { dotenvLanguage } from './remoteEditorConfigLanguage';
import { nginxLanguage } from './remoteEditorNginxLanguage';

export type RemoteEditorLanguageChoice = 'auto' | 'plain' | 'nginx' | 'properties' | 'dotenv' | 'shell' | 'json' | 'yaml';

const dockerfile = LanguageDescription.matchFilename(languages, 'Dockerfile');
const shell = LanguageDescription.matchFilename(languages, 'deploy.sh');
const properties = languages.find((language) => language.name === 'Properties files');
const json = languages.find((language) => language.name === 'JSON');
const yaml = languages.find((language) => language.name === 'YAML');

const dotenv = LanguageDescription.of({
  name: 'Dotenv',
  load: async () => new LanguageSupport(dotenvLanguage),
});
const nginx = LanguageDescription.of({
  name: 'Nginx',
  load: async () => new LanguageSupport(nginxLanguage),
});

function alias(name: string, description: LanguageDescription | null | undefined): LanguageDescription | null {
  return description ? LanguageDescription.of({ name, load: () => description.load() }) : null;
}

const dockerfileVariants = alias('Dockerfile', dockerfile);
const makefile = alias('Makefile', shell);
const genericConfig = alias('Properties files', properties);

export function remoteEditorLanguageDescription(path: string, choice: RemoteEditorLanguageChoice = 'auto'): LanguageDescription | null {
  if (choice === 'plain') return null;
  if (choice === 'nginx') return nginx;
  if (choice === 'properties') return genericConfig;
  if (choice === 'dotenv') return dotenv;
  if (choice === 'shell') return shell;
  if (choice === 'json') return json ?? null;
  if (choice === 'yaml') return yaml ?? null;

  const name = path.split('/').pop() || path;
  if (/^(?:\.env(?:\..+)?|.+\.env(?:\..+)?)$/i.test(name)) return dotenv;
  if (/^(?:Dockerfile(?:\..+)?|.+\.dockerfile)$/i.test(name)) return dockerfileVariants;
  if (/^Makefile$/i.test(name)) return makefile;
  if (/^nginx.*\.conf$/i.test(name) || /\/nginx\/(?:conf\.d|sites-(?:available|enabled))\//i.test(path)) return nginx;

  const detected = LanguageDescription.matchFilename(languages, name);
  if (detected?.name === 'Nginx') return nginx;
  if (detected) return detected;
  if (/\.conf$/i.test(name)) return genericConfig;
  return null;
}
