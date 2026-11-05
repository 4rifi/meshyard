import { mkdir, copyFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
const target = 'aarch64-apple-darwin';
function run(command, args, cwd = process.cwd()) {
  const result = spawnSync(command, args, { cwd, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
if (!process.env.CARGO_TARGET_DIR?.startsWith('/Volumes/TokensAbuse/')) {
  throw new Error('Source the APFS env.sh before building');
}
run('cargo', [
  'build',
  '--locked',
  '--release',
  '--target',
  target,
  '-p',
  'meshyard-hook',
]);
await mkdir('app/src-tauri/binaries', { recursive: true });
await copyFile(
  resolve(process.env.CARGO_TARGET_DIR, target, 'release/meshyard-hook'),
  `app/src-tauri/binaries/meshyard-hook-${target}`,
);
run('node', ['scripts/check-strict-bundle.mjs', '--self-test']);
run(
  resolve('node_modules/.bin/tauri'),
  ['build', '--target', target, '--bundles', 'app,dmg'],
  resolve('app'),
);
run('node', [
  'scripts/check-strict-bundle.mjs',
  resolve(
    process.env.CARGO_TARGET_DIR,
    target,
    'release/bundle/macos/Meshyard.app',
  ),
]);
