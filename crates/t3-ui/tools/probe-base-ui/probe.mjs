#!/usr/bin/env node
// Measures Base UI 1.5's interaction behavior (the library under the fork's coss components)
// in headless Chromium, so t3-ui reproduces it from facts instead of reading hook source.
//
// Usage: node crates/t3-ui/tools/probe-base-ui/probe.mjs [referenceRepo]
// Needs the reference repo's node_modules (esbuild, react, @base-ui/react, playwright-core).
// Prints a JSON report: keyboard navigation, typeahead, submenu keys, tooltip timing and
// delay groups, and popup placement near window edges.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";

const ref = path.resolve(process.argv[2] ?? path.join(os.homedir(), "L-Projects/t3code-again"));
const pnpm = path.join(ref, "node_modules/.pnpm");
const pick = (prefix) => fs.readdirSync(pnpm).filter((d) => d.startsWith(prefix)).sort().at(-1);
const esbuild = createRequire(path.join(pnpm, pick("esbuild@"), "node_modules/esbuild/package.json"))("esbuild");
const { chromium } = createRequire(path.join(ref, "apps/desktop/package.json"))("playwright-core");

const here = path.dirname(new URL(import.meta.url).pathname);
const bundle = await esbuild.build({
  entryPoints: [path.join(here, "page.jsx")],
  bundle: true,
  write: false,
  format: "iife",
  jsx: "automatic",
  define: { "process.env.NODE_ENV": '"production"' },
  nodePaths: [path.join(ref, "apps/web/node_modules")],
});
const html = `<!doctype html><html><body style="margin:0;font:14px sans-serif"><div id="root"></div><script>${bundle.outputFiles[0].text}</script></body></html>`;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 900, height: 600 } });
await page.setContent(html);
await page.waitForSelector("#menu");

const report = {};
const highlighted = (popup) =>
  page.evaluate((sel) => {
    const el = document.querySelector(`${sel} [data-highlighted]:not([aria-hidden])`);
    return el ? el.textContent : null;
  }, popup);
const isOpen = (sel) => page.evaluate((s) => !!document.querySelector(s), sel);
const focused = () => page.evaluate(() => document.activeElement?.id || document.activeElement?.textContent);
const press = async (key, popup = "#menu-popup") => {
  await page.keyboard.press(key);
  await page.waitForTimeout(60);
  return highlighted(popup);
};
const closeAll = async () => {
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await page.waitForTimeout(100);
};

// --- Menu: opening keys -----------------------------------------------------------------
for (const key of ["Enter", "Space", "ArrowDown", "ArrowUp"]) {
  await page.focus("#menu");
  await page.keyboard.press(key === "Space" ? " " : key);
  await page.waitForTimeout(150);
  report[`menu open via ${key}: highlighted`] = await highlighted("#menu-popup");
  report[`menu open via ${key}: open`] = await isOpen("#menu-popup");
  await closeAll();
}
await page.click("#menu");
await page.waitForTimeout(150);
report["menu open via click: highlighted"] = await highlighted("#menu-popup");

// --- Menu: navigation -------------------------------------------------------------------
const seq = [];
for (const key of ["ArrowDown", "ArrowDown", "ArrowDown", "ArrowDown", "ArrowDown", "ArrowDown", "ArrowDown", "ArrowUp", "Home", "ArrowUp", "End", "End"]) {
  seq.push(`${key}->${await press(key)}`);
}
report["menu nav sequence (from click)"] = seq;
report["menu typeahead b"] = await press("b");
report["menu typeahead b again"] = await press("b");
await page.waitForTimeout(800);
report["menu typeahead bl"] = (await press("b"), await press("l"));
await page.waitForTimeout(800);
report["menu typeahead c (disabled Cherry)"] = await press("c");
// Submenu
await press("End");
report["submenu ArrowRight: sub open"] = (await page.keyboard.press("ArrowRight"), await page.waitForTimeout(200), await isOpen("#menu-sub"));
report["submenu ArrowRight: highlighted"] = await highlighted("#menu-sub");
report["submenu ArrowLeft: sub open"] = (await page.keyboard.press("ArrowLeft"), await page.waitForTimeout(200), await isOpen("#menu-sub"));
report["submenu ArrowLeft: parent highlighted"] = await highlighted("#menu-popup");
report["submenu Enter opens"] = (await page.keyboard.press("Enter"), await page.waitForTimeout(200), await isOpen("#menu-sub"));
report["submenu Enter highlighted"] = await highlighted("#menu-sub");
await page.keyboard.press("Escape");
await page.waitForTimeout(150);
report["Escape in submenu: sub open / root open"] = [await isOpen("#menu-sub"), await isOpen("#menu-popup")];
await page.keyboard.press("Escape");
await page.waitForTimeout(150);
report["Escape: root open, focus"] = [await isOpen("#menu-popup"), await focused()];
// Pointer then keyboard
await page.click("#menu");
await page.waitForTimeout(150);
const box = await page.locator("#menu-popup >> text=Blueberry").boundingBox();
await page.mouse.move(box.x + 10, box.y + 10);
await page.waitForTimeout(100);
report["hover highlights"] = await highlighted("#menu-popup");
report["ArrowDown after hover"] = await press("ArrowDown");
await page.mouse.move(5, 5);
await page.waitForTimeout(100);
report["pointer leaves popup: highlighted"] = await highlighted("#menu-popup");
const cherry = await page.locator("#menu-popup >> text=Cherry").boundingBox();
await page.mouse.move(cherry.x + 10, cherry.y + 10);
await page.waitForTimeout(100);
report["hover disabled: highlighted"] = await highlighted("#menu-popup");
report["Space activates (menu closes)"] = (await press("Home"), await page.keyboard.press(" "), await page.waitForTimeout(150), !(await isOpen("#menu-popup")));
report["Tab with menu open: open after"] = (await page.click("#menu"), await page.waitForTimeout(150), await page.keyboard.press("Tab"), await page.waitForTimeout(150), await isOpen("#menu-popup"));
await closeAll();

// --- Select ---------------------------------------------------------------------------
for (const key of ["Enter", "ArrowDown", "ArrowUp", "Space"]) {
  await page.focus("#select");
  await page.keyboard.press(key === "Space" ? " " : key);
  await page.waitForTimeout(200);
  report[`select open via ${key}: highlighted`] = await highlighted("#select-popup");
  await closeAll();
}
await page.click("#select");
await page.waitForTimeout(250);
report["select click: highlighted"] = await highlighted("#select-popup");
const tseq = [];
for (const key of ["ArrowDown", "ArrowDown", "ArrowDown", "ArrowDown", "ArrowDown", "Home", "ArrowUp"]) {
  tseq.push(`${key}->${await press(key, "#select-popup")}`);
}
report["select nav sequence"] = tseq;
const trig = await page.locator("#select").boundingBox();
const val = await page.locator("#select-value").boundingBox();
const pop = await page.locator("#select-popup").boundingBox();
const sel = await page.locator("#select-popup [role=option][data-selected]").boundingBox();
report["select alignItemWithTrigger geometry"] = { trigger: trig, value: val, popup: pop, selectedItem: sel };
await page.keyboard.press("Enter");
await page.waitForTimeout(150);
report["select Enter commits and closes"] = [await page.textContent("#select-value"), await isOpen("#select-popup"), await focused()];
await page.focus("#select");
await page.keyboard.press("c");
await page.waitForTimeout(150);
report["select closed typeahead c"] = [await page.textContent("#select-value"), await isOpen("#select-popup")];
await closeAll();

// --- Tooltips ---------------------------------------------------------------------------
async function hoverTiming(id) {
  const b = await page.locator(`#${id}`).boundingBox();
  await page.mouse.move(b.x + 5, b.y + 5);
  const t0 = Date.now();
  await page.waitForSelector(`#${id}-popup`, { timeout: 3000 });
  return Date.now() - t0;
}
await page.mouse.move(800, 50);
await page.waitForTimeout(1200);
report["tooltip open delay ms (no provider)"] = await hoverTiming("tip-a");
const tb = await page.locator("#tip-b").boundingBox();
await page.mouse.move(tb.x + 5, tb.y + 5);
let t0 = Date.now();
await page.waitForSelector("#tip-b-popup", { timeout: 3000 });
report["tooltip move to neighbour ms (no provider)"] = Date.now() - t0;
report["tooltip neighbour instant attr (no provider)"] = await page.getAttribute("#tip-b-popup", "data-instant");
await page.mouse.move(800, 50);
t0 = Date.now();
await page.waitForSelector("#tip-b-popup", { state: "detached", timeout: 3000 });
report["tooltip close after leave ms"] = Date.now() - t0;
await page.waitForTimeout(1200);
report["tooltip open delay ms (provider)"] = await hoverTiming("grp-a");
const gb = await page.locator("#grp-b").boundingBox();
await page.mouse.move(gb.x + 5, gb.y + 5);
t0 = Date.now();
await page.waitForSelector("#grp-b-popup", { timeout: 3000 });
report["tooltip move to neighbour ms (provider)"] = Date.now() - t0;
report["tooltip neighbour instant attr (provider)"] = await page.getAttribute("#grp-b-popup", "data-instant");
await page.mouse.move(800, 50);
await page.waitForTimeout(250);
await page.mouse.move(gb.x + 5, gb.y + 5);
t0 = Date.now();
await page.waitForSelector("#grp-b-popup", { timeout: 3000 });
report["tooltip reopen after 250ms away (provider)"] = Date.now() - t0;
await page.mouse.move(800, 50);
await page.waitForTimeout(600);
await page.mouse.move(gb.x + 5, gb.y + 5);
t0 = Date.now();
await page.waitForSelector("#grp-b-popup", { timeout: 3000 });
report["tooltip reopen after 600ms away (provider)"] = Date.now() - t0;
await page.mouse.move(800, 50);

// --- Edge placement ---------------------------------------------------------------------
await page.waitForTimeout(400);
await page.click("#edge-menu");
await page.waitForTimeout(200);
report["edge menu: trigger / popup"] = [await page.locator("#edge-menu").boundingBox(), await page.locator("#edge-menu-popup").boundingBox()];
report["edge menu side"] = await page.getAttribute("#edge-menu-popup", "data-side");
await closeAll();
await page.click("#edge-select");
await page.waitForTimeout(250);
report["edge select: trigger / popup"] = [await page.locator("#edge-select").boundingBox(), await page.locator("#edge-select-popup").boundingBox()];
report["edge select side"] = await page.getAttribute("#edge-select-popup", "data-side");

console.log(JSON.stringify(report, null, 2));
await browser.close();
