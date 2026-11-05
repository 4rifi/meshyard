import { readFile, writeFile } from 'node:fs/promises';
import { chromium } from '/Volumes/TokensAbuse/tooling/visual/node_modules/playwright-core/index.mjs';
const font = (await readFile('app/src/assets/fonts/PixelifySans.ttf')).toString(
  'base64',
);
let svg = await readFile('assets/dmg-background.svg', 'utf8');
svg = svg.replace(
  '<defs>',
  `<defs><style>@font-face{font-family:Pixelify;src:url(data:font/ttf;base64,${font})}text{font-family:Pixelify,monospace}</style>`,
);
const b = await chromium.launch({
  executablePath:
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  env: { ...process.env, HOME: '/Volumes/TokensAbuse/worker-home' },
});
const p = await b.newPage({
  viewport: { width: 640, height: 440 },
  deviceScaleFactor: 1,
});
await p.setContent('<style>body{margin:0}</style>' + svg);
await p.evaluate(() => document.fonts.ready);
await p.locator('svg').screenshot({ path: 'assets/dmg-background.png' });
await b.close();
