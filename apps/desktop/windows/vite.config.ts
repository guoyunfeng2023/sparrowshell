import fs from 'node:fs';
import path from 'node:path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const clientConfigPath = path.resolve(process.cwd(), 'config.example.json');
if (!process.env.VITE_SPARROWSHELL_API_BASE_URL && fs.existsSync(clientConfigPath)) {
  const clientConfig = JSON.parse(fs.readFileSync(clientConfigPath, 'utf8')) as { apiBaseUrl?: unknown };
  if (typeof clientConfig.apiBaseUrl !== 'string' || !clientConfig.apiBaseUrl.trim()) {
    throw new Error(`${clientConfigPath} 缺少有效的 apiBaseUrl`);
  }
  process.env.VITE_SPARROWSHELL_API_BASE_URL = clientConfig.apiBaseUrl;
}

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 4179, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_'],
  build: { target: 'es2022', chunkSizeWarningLimit: 800 },
});
