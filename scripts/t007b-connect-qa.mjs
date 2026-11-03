// Run in an authorized NORMAL shell. Never opens a native app or owner config.
import { build } from 'vite';
import { chromium } from '/Volumes/MeshyardProjects/Meshyard-build/csp-regression-2026-11-03/tooling/node_modules/playwright-core/index.mjs';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir, mkdtemp } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { extname, resolve } from 'node:path';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
const repo = fileURLToPath(new URL('..', import.meta.url)).replace(/\/$/, '');
const base =
  '/Volumes/MeshyardProjects/Meshyard-build/orchestration/t007b-connect-evidence';
const arg = process.argv.indexOf('--output-dir');
const destination =
  arg >= 0 ? process.argv[arg + 1] : `${base}/browser-${Date.now()}`;
assert.ok(
  destination && resolve(destination).startsWith(`${base}/`),
  'Use a fresh folder inside the connect evidence root',
);
await mkdir(base, { recursive: true });
await mkdir(destination); // exclusive: preserve every earlier success/failure
assert.equal(
  process.env.CARGO_TARGET_DIR,
  '/Volumes/MeshyardProjects/Meshyard-build/targets/t-007b-local-agent-integration',
);
assert.ok(process.env.TMPDIR.startsWith('/Volumes/MeshyardProjects/'));
const head = () =>
  execFileSync('git', ['rev-parse', 'HEAD'], {
    cwd: repo,
    encoding: 'utf8',
  }).trim();
async function inputs() {
  const paths = execFileSync(
    'git',
    ['ls-files', '--cached', '--others', '--exclude-standard', '-z'],
    { cwd: repo, encoding: 'utf8' },
  )
    .split('\0')
    .filter(Boolean);
  return Object.fromEntries(
    await Promise.all(
      [...new Set(paths)].map(async (path) => [
        path,
        createHash('sha256')
          .update(await readFile(`${repo}/${path}`))
          .digest('hex'),
      ]),
    ),
  );
}
const sourceHashes = await inputs();
const csp = JSON.parse(
  await readFile(`${repo}/app/src-tauri/tauri.conf.json`, 'utf8'),
).app.security.csp;
const result = {
  evidenceLevel:
    'BROWSER MOCK IPC ONLY; actual App/client/Tauri listener mock/renderer. No native, persistence, actual hook installation, real CLI or owner acceptance.',
  sourceHead: head(),
  sourceHashes,
  fixtureHashes: Object.fromEntries(
    Object.entries(sourceHashes).filter(([path]) =>
      path.startsWith('app/src/dev/'),
    ),
  ),
  buildOnly: process.argv.includes('--build-only'),
  csp,
  startedAt: new Date().toISOString(),
  cases: [],
  errors: [],
  screenshots: [],
};
const started = performance.now();
let browser;
let server;
const record = (file, data) =>
  writeFile(`${destination}/${file}`, JSON.stringify(data, null, 2) + '\n');
try {
  // Default production graph must exclude every mock entry and fixture API.
  const production = await build({
    configFile: `${repo}/app/vite.config.ts`,
    build: { outDir: `${destination}/production`, emptyOutDir: false },
  });
  const productionFiles = (
    Array.isArray(production) ? production : [production]
  )
    .flatMap((bundle) => bundle.output)
    .map((output) => output.fileName);
  for (const file of productionFiles.filter((file) => file.endsWith('.js'))) {
    const source = await readFile(`${destination}/production/${file}`, 'utf8');
    assert.ok(
      !source.includes('connectFixture') &&
        !source.includes('fixture-plan-') &&
        !source.includes('mockIPC'),
      'Production excludes fixture',
    );
  }
  const mock = await build({
    configFile: `${repo}/app/vite.config.ts`,
    mode: 'browser-mock-ipc',
    build: {
      outDir: `${destination}/mock`,
      emptyOutDir: false,
      rollupOptions: { input: `${repo}/app/src/dev/browser-mock.html` },
    },
  });
  const mockFiles = (Array.isArray(mock) ? mock : [mock])
    .flatMap((bundle) => bundle.output)
    .map((output) => output.fileName);
  result.bundleHashes = Object.fromEntries(
    await Promise.all(
      [
        ...productionFiles.map((file) => `production/${file}`),
        ...mockFiles.map((file) => `mock/${file}`),
      ].map(async (file) => [
        file,
        createHash('sha256')
          .update(await readFile(`${destination}/${file}`))
          .digest('hex'),
      ]),
    ),
  );
  await record('build.json', {
    ...result,
    buildOnly: true,
    productionFiles,
    mockFiles,
  });
  if (!process.argv.includes('--build-only')) {
    server = createServer(async (request, response) => {
      try {
        const pathname = new URL(request.url, 'http://localhost').pathname;
        if (pathname === '/favicon.ico') {
          response.writeHead(204);
          response.end();
          return;
        }
        const path = resolve(`${destination}/mock`, `.${pathname}`);
        assert.ok(path.startsWith(`${destination}/mock/`));
        response.writeHead(200, {
          'Content-Security-Policy': csp,
          'Content-Type':
            {
              '.html': 'text/html',
              '.js': 'text/javascript',
              '.css': 'text/css',
            }[extname(path)] ?? 'application/octet-stream',
          'Cache-Control': 'no-store',
        });
        response.end(await readFile(path));
      } catch {
        response.writeHead(404);
        response.end();
      }
    });
    await new Promise((yes, no) => {
      server.once('error', no);
      server.listen(0, '127.0.0.1', yes);
    });
    const origin = `http://127.0.0.1:${server.address().port}`;
    // Fresh persistent profile/context per width AND clock/timing mode.
    for (const width of [960, 375])
      for (const mode of [
        'keyboard-flow',
        'clock-only-stale',
        'reduced-motion',
      ]) {
        const entry = { width, mode, errors: [], requests: [], frames: [] };
        result.cases.push(entry);
        const profile = await mkdtemp(
          `${destination}/chrome-${width}-${mode}-`,
        );
        browser = await chromium.launchPersistentContext(profile, {
          executablePath:
            '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
          headless: true,
          args: ['--disable-background-networking'],
          timeout: 30000,
        });
        entry.browserVersion =
          browser.browser()?.version() ?? 'persistent Chromium';
        const page = await browser.newPage();
        page.setDefaultTimeout(10000);
        await page.setViewportSize({ width, height: 800 });
        await page.emulateMedia({
          reducedMotion: mode === 'reduced-motion' ? 'reduce' : 'no-preference',
        });
        page.on('pageerror', (error) => entry.errors.push(error.message));
        page.on('console', (message) => {
          if (message.type() === 'error') entry.errors.push(message.text());
        });
        page.on('requestfailed', (request) =>
          entry.errors.push(`Request failed: ${request.url()}`),
        );
        page.on('request', (request) => {
          entry.requests.push(request.url());
        });
        await page.addInitScript(() => {
          window.cspViolations = [];
          document.addEventListener('securitypolicyviolation', (event) =>
            window.cspViolations.push(event.effectiveDirective),
          );
        });
        if (mode === 'clock-only-stale') {
          await page.clock.install();
          await page.clock.pauseAt(new Date());
        }
        const scenario = mode === 'keyboard-flow' ? 'setup' : 'connected';
        await page.goto(
          `${origin}/src/dev/browser-mock.html?scenario=${scenario}`,
          { waitUntil: 'networkidle' },
        );
        const panel = page.locator('#connection-panel');
        const button = (name) =>
          page.getByRole('button', { name, exact: true });
        const fixture = () =>
          page.evaluate(() => window.connectFixture.snapshot());
        const tabTo = async (locator) => {
          for (let index = 0; index < 100; index++) {
            if (
              await locator.evaluate(
                (element) => element === document.activeElement,
              )
            )
              return;
            await page.keyboard.press('Tab');
          }
          throw new Error(
            `Keyboard target not reachable: ${await locator.getAttribute('id')}`,
          );
        };
        const press = async (name) => {
          const target = button(name);
          await tabTo(target);
          await page.keyboard.press('Enter');
        };
        const capture = async (name) => {
          const snapshot = await page.evaluate(() => {
            const canvas = document
              .querySelector('canvas')
              ?.getBoundingClientRect();
            return {
              scrollWidth: document.documentElement.scrollWidth,
              viewport: innerWidth,
              canvas: canvas ? { x: canvas.x, y: canvas.y } : null,
              csp: window.cspViolations,
              focus:
                document.activeElement?.id ||
                document.activeElement?.textContent?.slice(0, 80),
            };
          });
          assert.ok(
            snapshot.scrollWidth <= snapshot.viewport,
            'No document overflow',
          );
          if (snapshot.canvas)
            for (const position of Object.values(snapshot.canvas))
              assert.equal(
                position,
                Math.floor(position),
                'Integer canvas origin',
              );
          assert.deepEqual(snapshot.csp, []);
          const file = `${mode}-${width}-${name}.png`;
          await page.screenshot({
            path: `${destination}/${file}`,
            fullPage: true,
          });
          entry.frames.push({ name, snapshot });
          result.screenshots.push(file);
        };
        if (mode === 'keyboard-flow') {
          await tabTo(page.locator('#workspace-name'));
          await page.keyboard.type('Browser mock yard');
          await tabTo(page.locator('#display-name'));
          await page.keyboard.type('Fixture owner');
          await press('Create workspace');
          await button('Connect Claude Code').waitFor();
          await press('Connect Claude Code');
          assert.equal(
            await page
              .locator('#connect-heading')
              .evaluate((el) => el === document.activeElement),
            true,
          );
          await press('Register one');
          assert.equal(
            await page
              .locator('#agent-label')
              .evaluate((el) => el === document.activeElement),
            true,
          );
          await page.keyboard.type('primary worker');
          await press('Register agent');
          await page
            .getByText('primary worker', { exact: true })
            .first()
            .waitFor();
          assert.equal(
            (await fixture()).workspace.agents[0].connection,
            'NotConnected',
          );
          await tabTo(page.locator('#agent-label'));
          await page.keyboard.type('selected worker');
          await press('Register agent');
          const selected = panel
            .getByRole('listitem')
            .filter({ hasText: 'selected worker' })
            .getByRole('button', { name: 'Select for hook', exact: true });
          await tabTo(selected);
          await page.keyboard.press('Enter');
          await page.waitForFunction(
            () =>
              window.connectFixture
                .snapshot()
                .workspace.agents.find((a) => a.label === 'selected worker')
                ?.bound,
          );
          await page.evaluate(() => window.connectFixture.addForeign());
          await page.waitForFunction(() =>
            document.body.textContent.includes('foreign agent'),
          );
          assert.equal(
            await panel.getByText('foreign agent', { exact: true }).count(),
            0,
            'Foreign agent excluded from selector',
          );
          await tabTo(page.locator('#agent-label'));
          await page.keyboard.type('keep draft');
          await press('Review settings');
          await panel
            .getByRole('heading', { name: 'Review hook settings' })
            .waitFor();
          assert.ok(
            (await panel.locator('pre').textContent()).includes(
              "'/fixture USB/meshyard-hook'",
            ),
          );
          const plan = (await fixture()).planned;
          const focusBefore = await page.evaluate(
            () => document.activeElement.textContent,
          );
          await page.evaluate(() => window.connectFixture.burst());
          await page.waitForTimeout(100);
          assert.equal((await fixture()).planned.plan_id, plan.plan_id);
          assert.equal(
            await page.locator('#agent-label').inputValue(),
            'keep draft',
          );
          assert.equal(
            await page.evaluate(() => document.activeElement.textContent),
            focusBefore,
            'Refresh does not steal focus',
          );
          await capture('review-before-cancel');
          await press('Cancel');
          assert.equal(
            (await fixture()).commands.filter((c) => c.command === 'hook_apply')
              .length,
            0,
          );
          await press('Review settings');
          await button('Install').waitFor();
          await page.evaluate(() =>
            window.connectFixture.failNext('HookPlanStale'),
          );
          await press('Install');
          await panel.getByRole('alert').waitFor();
          assert.ok((await panel.textContent()).includes('HookPlanStale'));
          assert.equal((await fixture()).workspace.hook.installed, false);
          await press('Cancel');
          await press('Review settings');
          await button('Install').waitFor();
          await page.evaluate(() => {
            window.connectFixture.delayReads(500);
            window.connectFixture.delayActions(250);
            window.connectFixture.burst();
          });
          const applyCount = (await fixture()).commands.filter(
            (c) => c.command === 'hook_apply',
          ).length;
          await press('Install');
          await page.waitForFunction(
            () =>
              document.querySelector('main').getAttribute('aria-busy') ===
              'true',
          );
          assert.equal(
            await button('Install').isDisabled(),
            true,
            'Install is locked during persistence',
          );
          await button('Install').evaluate((element) => element.click());
          assert.equal(
            (await fixture()).commands.filter((c) => c.command === 'hook_apply')
              .length,
            applyCount + 1,
            'Duplicate Install blocked',
          );
          await page.waitForFunction(
            () => window.connectFixture.snapshot().workspace.hook.installed,
          );
          await page.evaluate(() => {
            window.connectFixture.delayReads(0);
            window.connectFixture.delayActions(0);
          });
          await page.waitForTimeout(600);

          assert.ok(
            (await panel.textContent()).includes('Waiting for first event'),
          );
          assert.ok(
            !(await panel.textContent()).includes('Connected (receiving)'),
          );
          await page.locator('canvas').waitFor();
          await page.evaluate(() => {
            window.keptCanvas = document.querySelector('canvas');
            window.keptTag = document.querySelector(
              '[data-agent-id="fixture-own-1"]',
            );
          });
          await page.evaluate(() => window.connectFixture.event('Thinking'));
          await panel
            .getByText('Connected (receiving)', { exact: true })
            .first()
            .waitFor();
          await page.waitForTimeout(3200);
          assert.ok(
            await page.evaluate(
              () =>
                window.keptCanvas === document.querySelector('canvas') &&
                window.keptTag ===
                  document.querySelector('[data-agent-id="fixture-own-1"]'),
            ),
            'Scene identity survives real App refresh',
          );
          await capture('connected');
          await press('Review hook removal');
          await panel
            .getByRole('heading', { name: 'Review hook removal' })
            .waitFor();
          await press('Cancel');
          assert.equal((await fixture()).workspace.hook.installed, true);
          await press('Review hook removal');
          await button('Remove hook').waitFor();
          await press('Remove hook');
          await page.waitForFunction(
            () => !window.connectFixture.snapshot().workspace.hook.installed,
          );
          await page.evaluate(() =>
            window.connectFixture.failNext('HookConfigInvalid'),
          );
          await press('Review settings');
          await panel.getByRole('alert').waitFor();
          assert.ok((await panel.textContent()).includes('HookConfigInvalid'));
          await capture('typed-error-after-remove');
          await press('Close connection panel');
          assert.ok(
            await button('Connect Claude Code').evaluate(
              (el) => el === document.activeElement,
            ),
          );
          const kind = page.locator('#agent-kind');
          await tabTo(kind);
          entry.codexSelection = { beforeEnd: await kind.inputValue() };
          await page.keyboard.press('End');
          entry.codexSelection.afterEnd = await kind.inputValue();
          // macOS does not reliably select the last native option with End.
          // Open the menu, advance to its final option and commit with Enter.
          await page.keyboard.press('Space');
          await page.keyboard.press('ArrowDown');
          await page.keyboard.press('Enter');
          entry.codexSelection.afterKeyboard = await kind.inputValue();
          assert.equal(
            entry.codexSelection.afterKeyboard,
            'Codex',
            'Keyboard must select Codex before registration',
          );
          await tabTo(page.locator('#agent-label'));
          await page.keyboard.press('ControlOrMeta+A');
          await page.keyboard.type('Codex unavailable');
          await press('Register agent');
          await page
            .getByText('Codex unavailable', { exact: true })
            .first()
            .waitFor();
          assert.equal(
            (await fixture()).workspace.agents.find(
              (agent) => agent.label === 'Codex unavailable',
            )?.kind,
            'Codex',
            'The registered fixture agent must actually be Codex',
          );
          assert.ok(
            (await page.locator('main').textContent()).includes(
              'Codex connection is not available yet.',
            ),
          );
          const codexRow = page
            .getByRole('listitem')
            .filter({ hasText: 'Codex unavailable' });
          assert.ok(
            (await codexRow.textContent()).includes(
              'Codex connection is not available yet.',
            ),
          );
          assert.equal(
            await codexRow.getByRole('button').count(),
            0,
            'Unsupported Codex has no Connect or selection button',
          );
          await capture('codex-unavailable');
        } else {
          await page.locator('canvas').waitFor();
          const tag = page.locator('[data-agent-id="fixture-clock-agent"]');
          await tag.waitFor();
          if (mode === 'clock-only-stale') {
            await page.clock.runFor(3200);
            const coordinates = await tag.evaluate((el) => [
              el.style.left,
              el.style.top,
            ]);
            await page.evaluate(() =>
              window.connectFixture.staleWithoutEvent(),
            );
            await page.clock.runFor(5001);
            await page.waitForFunction(() =>
              document
                .querySelector('[data-agent-id="fixture-clock-agent"]')
                .title.includes('Unknown'),
            );
            assert.deepEqual(
              await tag.evaluate((el) => [el.style.left, el.style.top]),
              coordinates,
              'Staleness preserves last station',
            );
            await capture('derived-stale-no-append');
          } else {
            await page.evaluate(() => window.connectFixture.event('Testing'));
            await page.waitForFunction(() =>
              document
                .querySelector('[data-agent-id="fixture-clock-agent"]')
                .title.includes('Testing'),
            );
            const coordinates = await tag.evaluate((el) => [
              el.style.left,
              el.style.top,
            ]);
            await page.waitForTimeout(100);
            assert.deepEqual(
              await tag.evaluate((el) => [el.style.left, el.style.top]),
              coordinates,
              'Reduced motion snaps',
            );
            await capture('reduced-motion-testing');
          }
        }
        assert.equal(
          (await fixture()).listeners,
          1,
          'Exactly one StrictMode event listener',
        );
        await page.evaluate(() => window.connectFixture.unmount());
        await page.waitForFunction(
          () => window.connectFixture.snapshot().listeners === 0,
        );
        assert.ok(
          entry.requests.every((url) => url.startsWith(origin)),
          'No external assets/data',
        );
        assert.deepEqual(entry.errors, []);
        entry.passed = true;
        await browser.close();
        browser = null;
      }
  }
  result.passed = true;
} catch (error) {
  result.errors.push({ message: error.message, stack: error.stack });
  result.passed = false;
  process.exitCode = 1;
} finally {
  await browser?.close();
  if (server?.listening) await new Promise((yes) => server.close(yes));
  result.elapsedSeconds = Number(
    ((performance.now() - started) / 1000).toFixed(3),
  );
  result.endHead = head();
  const after = await inputs();
  result.sourceChangedDuringRun = Object.keys({
    ...sourceHashes,
    ...after,
  }).filter((key) => sourceHashes[key] !== after[key]);
  if (
    result.endHead !== result.sourceHead ||
    result.sourceChangedDuringRun.length
  ) {
    result.passed = false;
    process.exitCode = 1;
  }
  await record('result.json', result);
  console.log(
    JSON.stringify({
      passed: result.passed,
      cases: result.cases.length,
      screenshots: result.screenshots.length,
      errors: result.errors,
      elapsedSeconds: result.elapsedSeconds,
      result: `${destination}/result.json`,
    }),
  );
}
