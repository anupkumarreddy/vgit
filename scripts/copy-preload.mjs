import { copyFile, mkdir } from 'node:fs/promises';

await mkdir(new URL('../dist-electron/', import.meta.url), { recursive: true });
await copyFile(
  new URL('../electron/preload.cjs', import.meta.url),
  new URL('../dist-electron/preload.cjs', import.meta.url)
);
