import { describe, expect, it } from 'vitest';
import { remoteEditorLanguageDescription } from './remoteEditorLanguage';

describe('remote editor language detection', () => {
  it.each([
    ['/opt/app/deploy.sh', 'Shell'],
    ['/opt/app/main.py', 'Python'],
    ['/opt/app/config.json', 'JSON'],
    ['/opt/app/config.yaml', 'YAML'],
    ['/opt/app/Main.java', 'Java'],
    ['/opt/app/index.html', 'HTML'],
    ['/opt/app/styles.css', 'CSS'],
    ['/opt/app/Dockerfile', 'Dockerfile'],
    ['/opt/app/Dockerfile.prod', 'Dockerfile'],
    ['/opt/app/api.dockerfile', 'Dockerfile'],
    ['/opt/app/Makefile', 'Makefile'],
    ['/opt/app/.env', 'Dotenv'],
    ['/opt/app/.env.production', 'Dotenv'],
    ['/opt/app/service.env', 'Dotenv'],
    ['/opt/app/app.properties', 'Properties files'],
    ['/opt/app/app.ini', 'Properties files'],
    ['/etc/nginx/nginx.conf', 'Nginx'],
    ['/etc/nginx/conf.d/default.conf', 'Nginx'],
    ['/etc/nginx/sites-enabled/app.conf', 'Nginx'],
    ['/etc/redis/redis.conf', 'Properties files'],
  ])('detects %s as %s', (path, language) => {
    expect(remoteEditorLanguageDescription(path)?.name).toBe(language);
  });

  it('loads the language parser only when needed', async () => {
    const description = remoteEditorLanguageDescription('/opt/app/config.json');
    expect(description).not.toBeNull();
    const support = await description!.load();
    expect(support.language.name).toBe('json');
  });

  it('loads the dotenv parser without changing other languages', async () => {
    const description = remoteEditorLanguageDescription('/opt/app/.env');
    const support = await description!.load();
    expect(support.language.name).toBe('dotenv');
  });

  it('loads the dedicated Nginx parser', async () => {
    const description = remoteEditorLanguageDescription('/etc/nginx/nginx.conf');
    const support = await description!.load();
    expect(support.language.name).toBe('nginx');
  });

  it('allows manual language selection and plain text', async () => {
    const description = remoteEditorLanguageDescription('/etc/redis/redis.conf', 'nginx');
    expect(description?.name).toBe('Nginx');
    expect((await description!.load()).language.name).toBe('nginx');
    expect(remoteEditorLanguageDescription('/etc/nginx/nginx.conf', 'plain')).toBeNull();
    expect(remoteEditorLanguageDescription('/opt/app/unknown.custom', 'json')?.name).toBe('JSON');
  });

  it('falls back to plain text for unknown extensions and extensionless files', () => {
    expect(remoteEditorLanguageDescription('/opt/app/notes.custom-unknown')).toBeNull();
    expect(remoteEditorLanguageDescription('/etc/hosts')).toBeNull();
  });
});
