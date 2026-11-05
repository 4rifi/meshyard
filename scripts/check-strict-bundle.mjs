import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';

// Locked iroh 1.3.0 defaults.rs and address_lookup/pkarr.rs, and
// iroh-dns 1.4.1 dns.rs define these production and staging services.
// Match the parent domain too, including future region relay subdomains.
const forbidden = ['iroh.link', 'n0.computer', 'pkarr.org'];
export function inspect(bytes, name) {
  const text = bytes.toString('latin1').toLowerCase();
  for (const host of forbidden) {
    if (text.includes(host))
      throw new Error(`${name}: forbidden service ${host}`);
  }
}
if (process.argv[2] === '--self-test') {
  for (const host of forbidden) {
    let rejected = false;
    try {
      inspect(Buffer.from(`\0https://dns.${host}/pkarr\0`), 'synthetic');
    } catch {
      rejected = true;
    }
    if (!rejected) throw new Error(`Guard missed ${host}`);
  }
  inspect(Buffer.from('local-only'), 'positive control');
  console.log(
    'PASS: every forbidden synthetic sample rejected; clean control accepted',
  );
} else {
  const root = process.argv[2];
  if (!root)
    throw new Error('Usage: check-strict-bundle.mjs <app> | --self-test');
  let count = 0;
  async function walk(dir) {
    for (const item of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, item.name);
      if (item.isDirectory()) await walk(path);
      else if (item.isFile()) {
        const bytes = await readFile(path);
        // Inspect every bundle file, including all executable and embedded assets.
        inspect(bytes, path);
        count++;
      }
    }
  }
  await walk(resolve(root));
  if (!count) throw new Error('Empty bundle');
  console.log(`PASS: ${count} bundle files contain no forbidden hosts`);
}
