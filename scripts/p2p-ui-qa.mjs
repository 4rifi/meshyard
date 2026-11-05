// Isolated browser IPC evidence. Never launches a native app or touches owner config.
import { build } from 'vite';
import { chromium } from '/Volumes/TokensAbuse/tooling/visual/node_modules/playwright-core/index.mjs';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { extname, resolve } from 'node:path';
import assert from 'node:assert/strict';
assert.ok(process.env.TMPDIR?.startsWith('/Volumes/TokensAbuse/'));
const out = '/Volumes/TokensAbuse/dist/screenshots/phase4';
const built = '/Volumes/TokensAbuse/tmp/phase4-browser-build';
await mkdir(out, { recursive: true });
await build({
  configFile: 'app/vite.config.ts',
  mode: 'browser-mock-ipc',
  build: {
    outDir: built,
    emptyOutDir: true,
    rollupOptions: { input: resolve('app/src/dev/browser-mock.html') },
  },
});
const csp = JSON.parse(await readFile('app/src-tauri/tauri.conf.json', 'utf8'))
  .app.security.csp;
const server = createServer(async (req, res) => {
  try {
    const path = resolve(
      built,
      '.' + new URL(req.url, 'http://localhost').pathname,
    );
    assert.ok(path.startsWith(built + '/'));
    const data = await readFile(path);
    res.writeHead(200, {
      'Content-Security-Policy': csp,
      'Content-Type':
        {
          '.html': 'text/html',
          '.js': 'text/javascript',
          '.css': 'text/css',
          '.ttf': 'font/ttf',
        }[extname(path)] ?? 'application/octet-stream',
    });
    res.end(data);
  } catch {
    res.writeHead(404);
    res.end();
  }
});
await new Promise((yes) => server.listen(0, '127.0.0.1', yes));
const browser = await chromium.launch({
  executablePath:
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--disable-background-networking'],
  env: { ...process.env, HOME: '/Volumes/TokensAbuse/worker-home' },
});
const manifest = {
  evidence:
    'Browser mock IPC only; all invitations are invalid synthetic strings. No native/real QR scan claim.',
  captures: [],
  errors: [],
};
try {
  for (const [width, height] of [
    [320, 800],
    [375, 800],
    [960, 680],
    [1120, 900],
  ]) {
    const context = await browser.newContext({
      viewport: { width, height },
      deviceScaleFactor: 2,
      reducedMotion: 'reduce',
    });
    const page = await context.newPage();
    page.on('pageerror', (e) => manifest.errors.push(e.message));
    for (const scenario of [
      'join',
      'joining',
      'peer-invite',
      'peer-pending',
      'peer-confirmed',
      'peer-rejected',
      'peer-error',
      'peer-disconnected',
    ]) {
      await page.goto(
        `http://127.0.0.1:${server.address().port}/src/dev/browser-mock.html?scenario=${scenario}`,
      );
      await page.evaluate(() => document.fonts.ready);
      if (scenario === 'joining') {
        await page
          .getByRole('heading', { name: 'Waiting for owner' })
          .waitFor();
      } else if (scenario === 'join')
        await page
          .getByRole('button', { name: 'Join existing workspace', exact: true })
          .click();
      else
        await page.getByRole('button', { name: 'Peers', exact: true }).click();
      await page.waitForTimeout(600);
      if (scenario === 'peer-invite') {
        await page
          .getByRole('button', { name: 'Create private invite', exact: true })
          .click();
        await page
          .getByRole('button', {
            name: 'Show private link and QR',
            exact: true,
          })
          .click();
        assert.equal(
          await page
            .getByRole('img', { name: 'Private invitation QR code' })
            .count(),
          1,
        );
      }
      await page.evaluate(() => scrollTo(0, 0));
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth > innerWidth,
      );
      assert.equal(overflow, false, `${scenario} at ${width}: page overflows`);
      const file = `${scenario}-${width}-mock.png`;
      if (width >= 960) {
        if (scenario !== 'join') {
          const stop = await page
            .getByRole('button', { name: 'Stop sharing', exact: true })
            .boundingBox();
          assert.ok(
            stop && stop.y >= 0 && stop.y + stop.height <= height,
            'Stop sharing must fit the desktop viewport',
          );
        }
        if (scenario === 'peer-invite') {
          const qr = await page
            .getByRole('img', { name: 'Private invitation QR code' })
            .boundingBox();
          assert.ok(
            qr && qr.y >= 0 && qr.y + qr.height <= height,
            'Entire QR must fit the desktop viewport',
          );
        }
      }
      await page.screenshot({ path: resolve(out, file), fullPage: true });
      // Full-page capture can settle layout/scroll anchoring after a live status refresh.
      await page.evaluate(() => {
        document.activeElement?.blur();
        window.scrollTo({ top: 0, left: 0, behavior: 'instant' });
      });
      await page.waitForTimeout(350);
      await page.evaluate(() =>
        window.scrollTo({ top: 0, left: 0, behavior: 'instant' }),
      );
      assert.equal(
        await page.evaluate(() => scrollY),
        0,
        'Viewport capture begins at the actual page top',
      );
      if (width >= 960 && scenario !== 'join') {
        const stop = await page
          .getByRole('button', { name: 'Stop sharing', exact: true })
          .boundingBox();
        assert.ok(
          stop && stop.y >= 0 && stop.y + stop.height <= height,
          'Stop remains visible at capture time',
        );
      }
      if (width >= 960 && scenario === 'peer-pending') {
        const match = await page
          .getByRole('button', { name: 'Words match', exact: true })
          .boundingBox();
        const reject = await page
          .getByRole('button', { name: 'Reject mismatch', exact: true })
          .boundingBox();
        assert.ok(
          match &&
            reject &&
            match.y >= 0 &&
            reject.y >= 0 &&
            match.y + match.height <= height &&
            reject.y + reject.height <= height,
          'SAS actions must fit the desktop viewport',
        );
      }
      await page.screenshot({
        path: resolve(out, file.replace('.png', '-viewport.png')),
        fullPage: false,
      });
      assert.equal(
        await page.evaluate(() => scrollY),
        0,
        'Capture preserves top scroll position',
      );
      manifest.captures.push({ file, width, height, dpr: 2, scenario });
    }
    await context.close();
  }
  assert.deepEqual(manifest.errors, []);
  await writeFile(
    resolve(out, 'manifest.json'),
    JSON.stringify(manifest, null, 2) + '\n',
  );
  console.log(
    `PASS: ${manifest.captures.length} mock peer/join/QR/SAS/error screenshots, no page errors or horizontal overflow`,
  );
} finally {
  await browser.close();
  await new Promise((done) => server.close(done));
}
