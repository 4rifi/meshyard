// Isolated browser IPC evidence. Never launches a native app or touches owner config.
import { build } from 'vite';
import { chromium } from '/Volumes/TokensAbuse/tooling/visual/node_modules/playwright-core/index.mjs';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { extname, resolve } from 'node:path';
import assert from 'node:assert/strict';
assert.ok(process.env.TMPDIR?.startsWith('/Volumes/TokensAbuse/'));
const out = '/Volumes/TokensAbuse/dist/screenshots/map';
const built = '/Volumes/TokensAbuse/tmp/map-browser-build';
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
    'Synthetic browser IPC map states; Fix is unsupported by hooks. Not native or live CLI evidence.',
  captures: [],
  errors: [],
};
const agent = (id, state, kind = 'ClaudeCode') => ({
  id,
  label: id,
  kind,
  state,
  is_you: true,
  provenance: { Inferred: { rule: 'Synthetic map QA' } },
  bound: true,
  stale: false,
  connection: 'Connected',
});
try {
  for (const [width, height, dpr] of [
    [960, 680, 1],
    [1120, 900, 2],
    [375, 800, 2],
    [320, 680, 1],
  ]) {
    const context = await browser.newContext({
      viewport: { width, height },
      deviceScaleFactor: dpr,
      reducedMotion: 'reduce',
    });
    const page = await context.newPage();
    page.on('pageerror', (e) => manifest.errors.push(e.message));
    await page.goto(
      `http://127.0.0.1:${server.address().port}/src/dev/browser-mock.html?scenario=agents`,
    );
    await page.locator('canvas').waitFor();
    await page.waitForTimeout(350);
    const capture = async (name) => {
      const file = `${name}-${width}-dpr${dpr}.png`;
      await page.screenshot({ path: resolve(out, file), fullPage: true });
      manifest.captures.push({ file, width, height, dpr });
    };
    await capture('two-stations');
    await page.evaluate(
      (agents) => window.connectFixture.setAgents(agents),
      [
        agent('Claude one', 'Building'),
        agent('Codex two', 'Building', 'Codex'),
        agent('Claude three', 'Building'),
        agent('Extra four', 'Building'),
      ],
    );
    await page.waitForTimeout(400);
    assert.equal(await page.locator('.agent-tag:not([hidden])').count(), 3);
    assert.equal(
      await page.locator('.station-label[data-station="Build"]').textContent(),
      'Build · 4',
    );
    assert.equal(await page.locator('.station-label').count(), 11);
    const viewport = page.locator('.yard-viewport');
    await viewport.focus();
    const initialPan = await viewport.evaluate((el) => ({
      x: el.scrollLeft,
      y: el.scrollTop,
      vertical: el.scrollHeight > el.clientHeight,
    }));
    if (width < 836) {
      for (let n = 0; n < 40; n++) {
        await page.keyboard.press('ArrowRight');
        await page.waitForTimeout(20);
      }
      assert.ok(
        await viewport.evaluate((el, x) => el.scrollLeft > x, initialPan.x),
        'Focused ArrowRight pans narrow map',
      );
      const testLabel = await page
        .locator('.station-label[data-station="Test"]')
        .boundingBox();
      const box = await viewport.boundingBox();
      assert.ok(
        testLabel &&
          box &&
          testLabel.x < box.x + box.width &&
          testLabel.x + testLabel.width > box.x,
        'Keyboard pan reveals the right-hand Test station',
      );
    }
    if (initialPan.vertical) {
      await page.keyboard.press('ArrowDown');
      await page.waitForTimeout(200);
      assert.ok(
        await viewport.evaluate((el, y) => el.scrollTop > y, initialPan.y),
        'Focused ArrowDown pans map vertically',
      );
    }
    await capture('three-visible-four-count');
    const labels = await page
      .locator('.agent-tag:not([hidden])')
      .evaluateAll((els) =>
        els.map((el) => {
          const r = el.getBoundingClientRect();
          return {
            x: r.x,
            y: r.y,
            w: r.width,
            h: r.height,
            font: parseFloat(getComputedStyle(el).fontSize) * 2,
          };
        }),
      );
    for (let i = 0; i < labels.length; i++)
      for (let j = i + 1; j < labels.length; j++) {
        const a = labels[i],
          b = labels[j];
        assert.ok(
          a.y + a.h <= b.y ||
            b.y + b.h <= a.y ||
            a.x + a.w <= b.x ||
            b.x + b.w <= a.x,
          'Names do not overlap',
        );
      }
    await page.getByRole('button', { name: 'Agent list', exact: true }).click();
    assert.ok(
      await page
        .getByRole('button', { name: 'Inspect Extra four', exact: true })
        .isVisible(),
    );
    await capture('overflow-accessible-list');
    await page
      .getByRole('button', { name: 'The yard', exact: true })
      .count()
      .then(async (n) => {
        if (n)
          await page
            .getByRole('button', { name: 'The yard', exact: true })
            .click();
        else
          await page.getByRole('button', { name: 'Yard', exact: true }).click();
      });
    await page.evaluate(
      (agents) => window.connectFixture.setAgents(agents),
      [
        agent('Synthetic Fix', 'Fixing'),
        agent('Codex test', 'Testing', 'Codex'),
      ],
    );
    await page.waitForTimeout(350);
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.evaluate(
      (agents) => window.connectFixture.setAgents(agents),
      [
        agent('Synthetic Fix', 'Thinking'),
        agent('Codex test', 'Testing', 'Codex'),
      ],
    );
    await page.waitForFunction(
      () => {
        const el = document.querySelector(
          '.agent-tag[data-agent-id="Synthetic Fix"]',
        );
        return (
          el &&
          Number(el.dataset.footY) >= 77 &&
          Number(el.dataset.footY) <= 88 &&
          Number(el.dataset.footX) >= 198 &&
          Number(el.dataset.footX) <= 202
        );
      },
      {},
      { polling: 'raf', timeout: 5000 },
    );
    // Freeze only this synthetic QA page after an actual renderer walk reaches the mask.
    await page.evaluate(() => {
      window.requestAnimationFrame = () => 0;
      document.querySelector('.yard-heading p').textContent =
        'Synthetic Fix walk behind monitor; hooks do not emit Fix';
    });
    await capture('synthetic-monitor-occlusion');
    await context.close();
  }
  assert.deepEqual(manifest.errors, []);
  await writeFile(
    resolve(out, 'manifest.json'),
    JSON.stringify(manifest, null, 2) + '\n',
  );
  console.log(
    `PASS ${manifest.captures.length} map captures, capacity/list, both sprite kinds, actual synthetic walk monitor transit`,
  );
} finally {
  await browser.close();
  await new Promise((done) => server.close(done));
}
