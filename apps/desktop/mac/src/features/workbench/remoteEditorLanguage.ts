import { LanguageDescription, LanguageSupport } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { dotenvLanguage } from './remoteEditorConfigLanguage';

const dockerfile = LanguageDescription.matchFilename(languages, 'Dockerfile');
const shell = LanguageDescription.matchFilename(languages, 'deploy.sh');
const properties = languages.find((language) => language.name === 'Properties files');

const dotenv = LanguageDescription.of({
  name: 'Dotenv',
  load: async () => new LanguageSupport(dotenvLanguage),
});

function alias(name: string, description: LanguageDescription | null | undefined): LanguageDescription | null {
  return description ? LanguageDescription.of({ name, load: () => description.load() }) : null;
}

const dockerfileVariants = alias('Dockerfile', dockerfile);
const makefile = alias('Makefile', shell);
const genericConfig = alias('Properties files', properties);

export function remoteEditorLanguageDescription(path: string): LanguageDescription | null {
  const name = path.split('/').pop() || path;
  if (/^(?:\.env(?:\..+)?|.+\.env(?:\..+)?)$/i.test(name)) return dotenv;
  if (/^(?:Dockerfile(?:\..+)?|.+\.dockerfile)$/i.test(name)) return dockerfileVariants;
  if (/^Makefile$/i.test(name)) return makefile;

  const detected = LanguageDescription.matchFilename(languages, name);
  if (detected) return detected;
  if (/\.conf$/i.test(name)) return genericConfig;
  return null;
}
