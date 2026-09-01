// Space-time worldtube figure: three heading-relative views of hat
// glider s21 captured from the live essay viewer in the paper (light)
// palette, composed into one labelled strip.
//
// Views (angle-test winners; azimuths relative to the glider heading
// 226.6°, "behind" = heading−180° = 46.6°):
//   (a) above    behind+80°  (az 126.6° = 2.210 rad), el +15° (0.262)
//   (b) below    behind+135° (az 181.6° = 3.170 rad), el −10° (−0.175)
//   (c) overhead behind+60°  (az 106.6° = 1.861 rad), el +75° (1.309)
// Frozen at generation ≈ 90 with fade 120, so the visible history is
// the run entire: the braided two-glider launch, the generation-42
// tail-collision vertex, and the survivor's straight climb. The capture drags from the settled auto-orient state
// (az 2.210, el 0.262); drag sensitivities are 0.008 rad/px (azimuth)
// and 0.006 rad/px (elevation).
//
// Prerequisites: the dev server (cd web && npm run dev) and a
// playwright install (npm i playwright && npx playwright install
// chromium) resolvable via NODE_PATH. Run via generate.sh.
import { chromium } from 'playwright';
import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const OUT = process.env.FIG_OUT ?? dirname(fileURLToPath(import.meta.url));
const URL_ = 'http://localhost:5173/essay/?tubetheme=light&tubefade=120';
const FREEZE_AT = 90;
const VIEWS = [
  ['above', 2.21, 0.262],
  ['below', 3.17, -0.175],
  ['overhead', 1.861, 1.309],
];
const LABELS = {
  above: '(a) oblique, above the present plane',
  below: '(b) from below the plane',
  overhead: '(c) overhead',
};

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1100, height: 850 },
  deviceScaleFactor: 2,
});
page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message));
await page.goto(URL_);
await page.locator('#worldtube').scrollIntoViewIfNeeded();
await page.waitForFunction(
  () => !document.querySelector('#worldtube .ep-loading'),
  null,
  { timeout: 120000 },
);
// Grow fast, freeze at the target generation.
await page.evaluate(() => {
  const s = document.querySelector('#worldtube .ep-speed');
  s.value = '30';
  s.dispatchEvent(new Event('input'));
});
await page.waitForFunction(
  (g) => Number(document.querySelector('#worldtube .ep-gen')?.textContent) >= g,
  FREEZE_AT,
  { timeout: 120000 },
);
// Instant in-page click: page.click's actionability checks take long
// enough at 30 gen/s to overshoot the freeze point by ~50 generations.
await page.evaluate(() => {
  document.querySelector('#worldtube .ep-play').click();
});
await page.waitForTimeout(2500); // settle auto-orient at the above view
const gen = await page.textContent('#worldtube .ep-gen');
console.log('frozen at generation', gen);

const box = await page.locator('#worldtube .st-canvas').boundingBox();
const cx = box.x + box.width / 2;
const cy = box.y + box.height / 2;
let az = 2.21;
let el = 0.262;

const shots = {};
for (const [name, taz, tel] of VIEWS) {
  const dx = -(taz - az) / 0.008;
  const dy = (tel - el) / 0.006;
  if (Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) {
    await page.mouse.move(cx, cy);
    await page.mouse.down();
    await page.mouse.move(cx + dx, cy + dy, { steps: 12 });
    await page.mouse.up();
    az = taz;
    el = tel;
    await page.waitForTimeout(600);
  }
  const data = await page.evaluate(
    () =>
      new Promise((resolve) => {
        requestAnimationFrame(() => {
          const gl = document.querySelector('#worldtube .st-canvas');
          const c = document.createElement('canvas');
          c.width = gl.width;
          c.height = gl.height;
          c.getContext('2d').drawImage(gl, 0, 0);
          resolve(c.toDataURL('image/png'));
        });
      }),
  );
  shots[name] = data;
  writeFileSync(join(OUT, `${name}.png`), Buffer.from(data.split(',')[1], 'base64'));
  console.log('captured', name);
}

// Compose the labelled strip in-page. The camera centres the tube's
// head, leaving the top and right of each raw frame empty (the tube
// trails to the lower left) — crop to the bottom-left 2/3 x 2/3.
const strip = await page.evaluate(async ({ shots, labels, order }) => {
  const imgs = {};
  for (const k of order) {
    imgs[k] = await new Promise((res) => {
      const im = new Image();
      im.onload = () => res(im);
      im.src = shots[k];
    });
  }
  const cropW = Math.round((imgs[order[0]].width * 2) / 3);
  const cropH = Math.round((imgs[order[0]].height * 2) / 3);
  const cropY = imgs[order[0]].height - cropH; // drop the top third
  const gap = 40;
  const labelH = 90;
  const c = document.createElement('canvas');
  c.width = order.length * cropW + (order.length - 1) * gap;
  c.height = cropH + labelH;
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#ffffff';
  ctx.fillRect(0, 0, c.width, c.height);
  ctx.fillStyle = '#333333';
  ctx.font = '44px Helvetica, Arial, sans-serif';
  ctx.textAlign = 'center';
  order.forEach((k, i) => {
    const x = i * (cropW + gap);
    ctx.drawImage(imgs[k], 0, cropY, cropW, cropH, x, 0, cropW, cropH);
    ctx.strokeStyle = '#c3c7cf';
    ctx.lineWidth = 2;
    ctx.strokeRect(x + 1, 1, cropW - 2, cropH - 2);
    ctx.fillText(labels[k], x + cropW / 2, cropH + 62);
  });
  return c.toDataURL('image/png');
}, { shots, labels: LABELS, order: VIEWS.map((v) => v[0]) });
writeFileSync(join(OUT, 'worldtube-strip.png'), Buffer.from(strip.split(',')[1], 'base64'));
console.log('composed worldtube-strip.png');
await browser.close();
