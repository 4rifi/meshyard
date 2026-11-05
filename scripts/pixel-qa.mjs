// Isolated browser IPC evidence. Never launches a native app or touches owner config.
import { build } from 'vite';
import { chromium } from '/Volumes/TokensAbuse/tooling/visual/node_modules/playwright-core/index.mjs';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { extname, resolve } from 'node:path';
import assert from 'node:assert/strict';
assert.ok(process.env.TMPDIR?.startsWith('/Volumes/TokensAbuse/'));
const out = '/Volumes/TokensAbuse/dist/screenshots';
const built = '/Volumes/TokensAbuse/tmp/pixel-browser-build';
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
    'Browser mock IPC, synthetic scenarios; not native or live CLI evidence',
  command: 'source /Volumes/TokensAbuse/env.sh\nnode scripts/pixel-qa.mjs',
  browser: browser.version(),
  captures: [],
  errors: [],
};
try {
  for (const [width, height, dpr] of [
    [1120, 900, 2],
    [375, 800, 2],
    [960, 680, 1],
    [320, 680, 2],
  ]) {
    const context = await browser.newContext({
      viewport: { width, height },
      deviceScaleFactor: dpr,
      reducedMotion: 'reduce',
    });
    const page = await context.newPage();
    page.on('pageerror', (e) => manifest.errors.push(e.message));
    await page.addInitScript(() => {
      window.visualCsp = [];
      document.addEventListener('securitypolicyviolation', (e) =>
        window.visualCsp.push(e.effectiveDirective),
      );
    });
    const open = async (scenario) => {
      await page.goto(
        `http://127.0.0.1:${server.address().port}/src/dev/browser-mock.html?scenario=${scenario}`,
      );
      await page.locator('main').waitFor();
      await page.evaluate(() => document.fonts.ready);
      await page.waitForTimeout(250);
    };
    const capture = async (scenario, target) => {
      if (target) await target.scrollIntoViewIfNeeded();
      const metrics = await page.evaluate(() => {
        const c = document.querySelector('canvas');
        const r = c?.getBoundingClientRect();
        return {
          viewport: { width: innerWidth, height: innerHeight },
          dpr: devicePixelRatio,
          scrollWidth: document.documentElement.scrollWidth,
          canvas:
            r && r.width > 0
              ? {
                  x: r.x,
                  y: r.y,
                  width: r.width,
                  height: r.height,
                  logicalWidth: c.width,
                }
              : null,
          csp: window.visualCsp,
        };
      });
      assert.ok(
        metrics.scrollWidth <= width,
        `No horizontal overflow: ${scenario}`,
      );
      assert.deepEqual(metrics.csp, []);
      assert.ok(
        await page.evaluate(() => document.fonts.check('16px "Pixelify Sans"')),
        'Local pixel font loaded',
      );
      if (
        metrics.canvas &&
        metrics.canvas.y * dpr !== Math.round(metrics.canvas.y * dpr)
      ) {
        console.log(
          JSON.stringify(
            await page.evaluate(() =>
              [...document.querySelectorAll('main *, aside')]
                .map((el) => {
                  const r = el.getBoundingClientRect();
                  const s = getComputedStyle(el);
                  return {
                    tag: el.tagName,
                    cls: el.className,
                    y: r.y,
                    h: r.height,
                    line: s.lineHeight,
                    font: s.fontSize,
                    top: s.top,
                  };
                })
                .filter(
                  (el) => !Number.isInteger(el.y) || !Number.isInteger(el.h),
                ),
            ),
            null,
            2,
          ),
        );
      }
      if (metrics.canvas) {
        assert.equal(
          metrics.canvas.x * dpr,
          Math.round(metrics.canvas.x * dpr),
        );
        assert.equal(
          metrics.canvas.y * dpr,
          Math.round(metrics.canvas.y * dpr),
        );
        assert.equal(
          metrics.canvas.width / metrics.canvas.logicalWidth,
          Math.floor(metrics.canvas.width / metrics.canvas.logicalWidth),
        );
      }
      const file = `${scenario}-${width}-dpr${dpr}.png`;
      await page.screenshot({ path: `${out}/${file}`, fullPage: true });
      const viewportFile = file.replace('.png', '-viewport.png');
      await page.screenshot({
        path: `${out}/${viewportFile}`,
        fullPage: false,
      });
      manifest.captures.push({
        scenario,
        file,
        ...metrics,
        evidence: 'browser mock IPC',
        viewportFile,
      });
    };
    for (const [scenario, name] of [
      ['setup', 'onboarding'],
      ['loading', 'loading'],
      ['empty', 'empty-yard'],
      ['error', 'error'],
      ['partial-create', 'repair-error'],
    ]) {
      await open(scenario);
      await capture(name);
    }
    await open('agents');
    await page.locator('canvas').waitFor();
    await capture('yard-two-kinds');
    await open('three-agents');
    await page.locator('canvas').waitFor();
    await capture('yard-three-agents');
    assert.ok(
      await page.getByText('3 registered agents', { exact: true }).isVisible(),
    );
    const canvasHeight = await page.locator('canvas').getAttribute('height');
    assert.equal(
      Number(canvasHeight),
      width >= 960 ? 192 : 384,
      'Three distinct states do not duplicate the complete map',
    );
    await page.getByRole('button', { name: 'Agent list', exact: true }).click();
    const inspectButton = page.getByRole('button', {
      name: 'Inspect Claude build',
      exact: true,
    });
    for (
      let tab = 0;
      tab < 100 &&
      !(await inspectButton.evaluate((el) => el === document.activeElement));
      tab++
    )
      await page.keyboard.press('Tab');
    assert.ok(
      await inspectButton.evaluate((el) => el === document.activeElement),
      'Inspector opener reachable with Tab',
    );
    await page.keyboard.press('Enter');
    await capture('inspector', page.locator('#inspector-heading'));
    await page
      .getByRole('button', { name: 'Close inspector', exact: true })
      .click();
    assert.ok(
      await page
        .getByRole('button', { name: 'Inspect Claude build', exact: true })
        .evaluate((el) => el === document.activeElement),
      'Inspector restores focus',
    );
    await capture('list', page.locator('#agents-heading'));
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    assert.ok(
      await page
        .getByRole('button', { name: 'Save display name', exact: true })
        .evaluate((el) => {
          const r = el.getBoundingClientRect();
          return r.top >= 0 && r.bottom <= innerHeight;
        }),
      'Profile save visible when Settings opens',
    );
    await capture('settings', page.locator('#members-heading'));
    await page.getByRole('button', { name: 'Yard', exact: true }).click();
    await page
      .getByRole('button', { name: 'Connect agents', exact: true })
      .click();
    await capture('connection-choice', page.locator('#connect-heading'));
    const assertActions = async (kind, approve) => {
      for (const name of [approve, 'Cancel', 'Close connection panel']) {
        const action = page.getByRole('button', { name, exact: true });
        const geometry = await action.evaluate((el) => {
          const r = el.getBoundingClientRect();
          const panel = el.closest('.connect-panel').getBoundingClientRect();
          return {
            top: r.top,
            bottom: r.bottom,
            panelTop: panel.top,
            panelBottom: panel.bottom,
            viewport: innerHeight,
          };
        });
        if (!(
          geometry.top >= Math.max(0, geometry.panelTop) &&
          geometry.bottom <= Math.min(geometry.viewport, geometry.panelBottom)
        )) {
          console.log(JSON.stringify({ width, kind, name, geometry }));
          await page.screenshot({
            path: `/Volumes/TokensAbuse/logs/pixel/action-failure-${width}.png`,
          });
        }
        assert.ok(
          geometry.top >= Math.max(0, geometry.panelTop) &&
            geometry.bottom <=
              Math.min(geometry.viewport, geometry.panelBottom),
          `${kind} ${name} must be visible without scrolling the review panel`,
        );
      }
    };
    const tabTo = async (locator) => {
      for (
        let count = 0;
        count < 100 &&
        !(await locator.evaluate((el) => el === document.activeElement));
        count++
      )
        await page.keyboard.press('Tab');
      assert.ok(
        await locator.evaluate((el) => el === document.activeElement),
        'Review action keyboard reachable',
      );
    };
    for (const kind of ['ClaudeCode', 'Codex']) {
      await page.locator('#connect-kind').selectOption(kind);
      await page
        .getByRole('button', { name: 'Review settings', exact: true })
        .click();
      await page
        .getByRole('heading', { name: 'Review hook settings', exact: true })
        .waitFor();
      const diff = await page.getByLabel('Exact settings diff').textContent();
      assert.ok(
        diff.includes(
          kind === 'Codex' ? '~/.codex/hooks.json' : '~/.claude/settings.json',
        ),
      );
      if (kind === 'Codex') assert.ok(!diff.includes('StopFailure'));
      await assertActions(kind, 'Install');
      await tabTo(page.getByRole('button', { name: 'Install', exact: true }));
      await page.keyboard.press('Tab');
      assert.ok(
        await page
          .getByRole('button', { name: 'Cancel', exact: true })
          .evaluate((el) => el === document.activeElement),
      );
      await capture(
        `installer-${kind}`,
        page.getByLabel('Exact settings diff'),
      );
      await page.keyboard.press('Enter');
      assert.equal(
        await page.evaluate(
          () =>
            window.connectFixture
              .snapshot()
              .commands.filter((c) => c.command === 'hook_apply').length,
        ),
        0,
        'Cancel never applies a plan',
      );
    }
    // Actual resize of the same persistent scene, with a real canvas and snap mode.
    await page
      .getByRole('button', { name: 'Close connection panel', exact: true })
      .click();
    await page.setViewportSize({ width: 640, height });
    await page.waitForTimeout(200);
    await page.setViewportSize({ width, height });
    await page.waitForTimeout(200);
    await capture('resized');
    await open('connected');
    await page.locator('canvas').waitFor();
    await page.evaluate(() => window.connectFixture.event('Testing'));
    const tag = page.locator('[data-agent-id="fixture-clock-agent"]');
    await page.waitForFunction(() =>
      document
        .querySelector('[data-agent-id="fixture-clock-agent"]')
        ?.title.includes('Testing'),
    );
    const snapped = await tag.evaluate((el) => [el.style.left, el.style.top]);
    await page.waitForTimeout(100);
    assert.deepEqual(
      await tag.evaluate((el) => [el.style.left, el.style.top]),
      snapped,
      'Reduced motion state change snaps without a walk',
    );
    await capture('reduced-motion');
    await open('installed');
    await page
      .getByRole('button', { name: 'Connect agents', exact: true })
      .click();
    for (const kind of ['ClaudeCode', 'Codex']) {
      await page.locator('#connect-kind').selectOption(kind);
      await page
        .getByRole('button', { name: 'Review hook removal', exact: true })
        .click();
      await page
        .getByRole('heading', { name: 'Review hook removal', exact: true })
        .waitFor();
      await assertActions(kind, 'Remove hook');
      await tabTo(
        page.getByRole('button', { name: 'Remove hook', exact: true }),
      );
      await page.keyboard.press('Tab');
      assert.ok(
        await page
          .getByRole('button', { name: 'Cancel', exact: true })
          .evaluate((el) => el === document.activeElement),
      );
      await capture(`removal-${kind}`, page.getByLabel('Exact settings diff'));
      await page.keyboard.press('Enter');
      assert.equal(
        await page.evaluate(
          () =>
            window.connectFixture
              .snapshot()
              .commands.filter((c) => c.command === 'hook_apply').length,
        ),
        0,
        'Removal cancellation never applies a plan',
      );
    }
    await context.close();
  }
  assert.deepEqual(manifest.errors, []);
  manifest.passed = true;
} catch (e) {
  manifest.passed = false;
  manifest.errors.push(e.stack);
  process.exitCode = 1;
} finally {
  await browser.close();
  await new Promise((yes) => server.close(yes));
  await writeFile(
    `${out}/manifest.json`,
    JSON.stringify(manifest, null, 2) + '\n',
  );
  console.log(
    JSON.stringify({
      passed: manifest.passed,
      captures: manifest.captures.length,
      errors: manifest.errors,
    }),
  );
}
