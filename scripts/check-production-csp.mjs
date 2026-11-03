import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

// Keep browser tooling and evidence external; no browser dependency belongs to the app.
const { chromium } = await import(process.env.MESHYARD_PLAYWRIGHT_MODULE);
const { PNG } = await import(process.env.MESHYARD_PNG_MODULE);
const root = fileURLToPath(new URL('../', import.meta.url));
const dist = resolve(root, 'app/dist');
const output = resolve(process.env.MESHYARD_CSP_OUTPUT);
await mkdir(output, { recursive: true });
const config = JSON.parse(
  await readFile(resolve(root, 'app/src-tauri/tauri.conf.json'), 'utf8'),
);
const csp = config.app.security.csp;
assert(!csp.includes("'unsafe-eval'"));
const probe = `
document.addEventListener('securitypolicyviolation', (event) => {
  if (event.blockedURI === 'eval') document.documentElement.dataset.evalViolation = event.effectiveDirective;
});
try {
  new Function('return 42')();
  document.documentElement.dataset.functionConstructor = 'allowed';
} catch (error) {
  document.documentElement.dataset.functionConstructor = error.name;
}
`;
const mime = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
};
const server = createServer(async (request, response) => {
  response.setHeader('Content-Security-Policy', csp);
  response.setHeader('Cache-Control', 'no-store');
  const pathname = new URL(request.url, 'http://localhost').pathname;
  if (pathname === '/__csp_probe.js') {
    response.setHeader('Content-Type', 'text/javascript');
    response.end(probe);
    return;
  }
  const path = resolve(dist, pathname === '/' ? 'index.html' : `.${pathname}`);
  if (!path.startsWith(`${dist}${sep}`)) {
    response.writeHead(403).end();
    return;
  }
  try {
    response.setHeader(
      'Content-Type',
      mime[extname(path)] ?? 'application/octet-stream',
    );
    response.end(await readFile(path));
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((resolveListen) =>
  server.listen(0, '127.0.0.1', resolveListen),
);
let browser;
const result = {
  csp,
  headless: true,
  requests: [],
  consoleErrors: [],
  pageErrors: [],
};
try {
  browser = await chromium.launch({
    executablePath: process.env.MESHYARD_CHROMIUM_EXECUTABLE,
    headless: true,
    args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
  });
  const context = await browser.newContext({
    viewport: { width: 960, height: 680 },
    deviceScaleFactor: 1,
    bypassCSP: false,
  });
  const page = await context.newPage();
  page.on('console', (message) => {
    if (message.type() === 'error') result.consoleErrors.push(message.text());
  });
  page.on('pageerror', (error) => result.pageErrors.push(error.message));
  const origin = `http://127.0.0.1:${server.address().port}`;
  await context.route('**/*', (route) => {
    const url = route.request().url();
    result.requests.push(url);
    return url.startsWith(`${origin}/`) ? route.continue() : route.abort();
  });
  const response = await page.goto(origin, { waitUntil: 'networkidle' });
  assert.equal(response.status(), 200);
  assert.equal(response.headers()['content-security-policy'], csp);
  await page.waitForFunction(
    () =>
      document.querySelector('canvas') ||
      document.querySelector('[role="status"]'),
  );
  result.canvasCount = await page.locator('canvas').count();
  result.unavailableFallback = await page.getByRole('status').count();
  result.heading = await page
    .getByRole('heading', { name: 'Meshyard', exact: true })
    .count();
  result.webgl = await page.locator('canvas').evaluateAll((canvases) => {
    return canvases.map((canvas) => {
      const gl = canvas.getContext('webgl2') ?? canvas.getContext('webgl');
      if (!gl) return null;
      const extension = gl.getExtension('WEBGL_debug_renderer_info');
      return {
        width: canvas.width,
        height: canvas.height,
        version: gl.getParameter(gl.VERSION),
        renderer: extension
          ? gl.getParameter(extension.UNMASKED_RENDERER_WEBGL)
          : gl.getParameter(gl.RENDERER),
      };
    });
  });
  result.screenshot = resolve(output, 'production-csp.png');
  const screenshot = await page.screenshot({
    path: result.screenshot,
    fullPage: true,
  });
  result.screenshotSha256 = createHash('sha256')
    .update(screenshot)
    .digest('hex');
  const pixels = PNG.sync.read(screenshot);
  result.tealPixels = 0;
  result.darkCanvasPixels = 0;
  for (let i = 0; i < pixels.data.length; i += 4) {
    const [r, g, b, a] = pixels.data.subarray(i, i + 4);
    if (r === 94 && g === 228 && b === 195 && a === 255) result.tealPixels++;
    if (r === 17 && g === 28 && b === 37 && a === 255)
      result.darkCanvasPixels++;
  }
  result.renderConsoleErrors = [...result.consoleErrors];
  // A same-origin script executes under CSP; DevTools evaluation itself can bypass eval bans.
  await page.evaluate(
    () =>
      new Promise((resolveProbe, rejectProbe) => {
        const script = document.createElement('script');
        script.src = '/__csp_probe.js';
        script.onload = resolveProbe;
        script.onerror = rejectProbe;
        document.head.appendChild(script);
      }),
  );
  await page.waitForFunction(
    () => document.documentElement.dataset.evalViolation === 'script-src',
  );
  result.functionConstructor = await page.evaluate(
    () => document.documentElement.dataset.functionConstructor,
  );
  result.evalViolation = await page.evaluate(
    () => document.documentElement.dataset.evalViolation,
  );
  assert.equal(result.functionConstructor, 'EvalError');
  assert.equal(result.evalViolation, 'script-src');
  assert.equal(result.heading, 1);
  assert.equal(
    result.canvasCount,
    1,
    'Production renderer must attach a canvas',
  );
  assert.equal(
    result.unavailableFallback,
    0,
    'Renderer fallback must be absent',
  );
  assert(result.webgl[0]?.version.startsWith('WebGL'));
  assert(
    result.tealPixels >= 10000,
    'Screenshot must contain the actual teal rectangle',
  );
  assert(
    result.darkCanvasPixels >= 100000,
    'Screenshot must contain the canvas background',
  );
  assert.deepEqual(result.renderConsoleErrors, []);
  assert.deepEqual(result.pageErrors, []);
  result.pass = true;
} catch (error) {
  result.pass = false;
  result.failure = String(error);
  process.exitCode = 1;
} finally {
  if (browser) await browser.close();
  await new Promise((resolveClose) => server.close(resolveClose));
  await writeFile(
    resolve(output, 'result.json'),
    `${JSON.stringify(result, null, 2)}\n`,
  );
  console.log(JSON.stringify(result, null, 2));
}
