// Layout metrics dump for the ChatMarkdown reference pages.
// Run in the page: (await import('/metrics.js')).dump('metrics-dark.json')
// PUTs the JSON next to the page (server.py accepts *.json) and returns a summary.
//
// Rects are CSS px relative to the .chat-markdown root's border box.
// rootPage gives the root's position in the 1440-wide page, so rects map onto
// the stitched screenshots (see captures in the JSON).

const round = (n) => Math.round(n * 1000) / 1000;

function rectOf(r, R) {
  return { x: round(r.left - R.left), y: round(r.top - R.top), width: round(r.width), height: round(r.height) };
}

function box(cs, prop) {
  return ["Top", "Right", "Bottom", "Left"].map((side) => cs[`${prop}${side}`]).join(" ");
}

function styleOf(el) {
  const cs = getComputedStyle(el);
  return {
    display: cs.display,
    position: cs.position,
    fontFamily: cs.fontFamily,
    fontSize: cs.fontSize,
    lineHeight: cs.lineHeight,
    fontWeight: cs.fontWeight,
    fontStyle: cs.fontStyle,
    letterSpacing: cs.letterSpacing,
    color: cs.color,
    backgroundColor: cs.backgroundColor,
    opacity: cs.opacity,
    margin: box(cs, "margin"),
    padding: box(cs, "padding"),
    borderWidth: ["Top", "Right", "Bottom", "Left"].map((s) => cs[`border${s}Width`]).join(" "),
    borderColor: ["Top", "Right", "Bottom", "Left"].map((s) => cs[`border${s}Color`]).join(" "),
    borderStyle: ["Top", "Right", "Bottom", "Left"].map((s) => cs[`border${s}Style`]).join(" "),
    borderRadius: cs.borderRadius,
    whiteSpace: cs.whiteSpace,
    textAlign: cs.textAlign,
    verticalAlign: cs.verticalAlign,
    textDecorationLine: cs.textDecorationLine,
    listStyleType: cs.listStyleType,
    listStylePosition: cs.listStylePosition,
    overflowX: cs.overflowX,
    maskImage: cs.maskImage === "none" ? undefined : cs.maskImage,
    transform: cs.transform === "none" ? undefined : cs.transform,
  };
}

// First rendered text of an element, skipping nested lists (for li) and
// whitespace-only nodes. Returns the rect of its first character.
function firstGlyphRect(el, R) {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      if (!node.data.trim()) return NodeFilter.FILTER_REJECT;
      if (node.parentElement.closest("ul,ol") !== el.closest("ul,ol")) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  const node = walker.nextNode();
  if (!node) return null;
  const offset = node.data.search(/\S/);
  const range = document.createRange();
  range.setStart(node, offset);
  range.setEnd(node, offset + 1);
  return { char: node.data[offset], ...rectOf(range.getBoundingClientRect(), R) };
}

function toRoman(n) {
  const map = [[10, "x"], [9, "ix"], [5, "v"], [4, "iv"], [1, "i"]];
  let out = "";
  for (const [v, s] of map) while (n >= v) { out += s; n -= v; }
  return out;
}

function markerText(li) {
  const cs = getComputedStyle(li);
  const index = [...li.parentElement.children].filter((c) => c.tagName === "LI").indexOf(li) + 1;
  switch (cs.listStyleType) {
    case "disc": return "• ";
    case "circle": return "◦ ";
    case "square": return "▪ ";
    case "decimal": return `${index}. `;
    case "lower-alpha": return `${String.fromCharCode(96 + index)}. `;
    case "lower-roman": return `${toRoman(index)}. `;
    default: return null;
  }
}

// Which face actually renders a computed stack: compare advance widths of a
// probe string against each candidate family with a generic fallback.
function usedFamily(el) {
  const cs = getComputedStyle(el);
  const probe = "mmmmmmmmmmlli0O@#WWiiiiiii.,;:";
  const ctx = document.createElement("canvas").getContext("2d");
  const measure = (family) => {
    ctx.font = `${cs.fontStyle} ${cs.fontWeight} 40px ${family}`;
    return ctx.measureText(probe).width;
  };
  const target = measure(cs.fontFamily);
  const families = cs.fontFamily.split(",").map((f) => f.trim());
  for (const family of families) {
    const generic = /^(serif|sans-serif|monospace|system-ui|ui-monospace|ui-sans-serif)$/.test(family);
    const withFallback = generic ? family : `${family}, ${/mono/i.test(cs.fontFamily) ? "serif" : "monospace"}`;
    if (Math.abs(measure(withFallback) - target) < 0.01) {
      if (generic) return family;
      // make sure the family exists (differs from the bare fallback)
      const bare = /mono/i.test(cs.fontFamily) ? "serif" : "monospace";
      if (Math.abs(measure(bare) - target) > 0.01) return family;
    }
  }
  return "unknown";
}

export async function dump(name) {
  await document.fonts.ready;
  const root = document.querySelector(".chat-markdown");
  const R = root.getBoundingClientRect();
  const all = [root, ...root.querySelectorAll("*")].filter((el) => {
    const svg = el.closest("svg");
    return !svg || svg === el; // keep <svg>, skip its children
  });
  const index = new Map(all.map((el, i) => [el, i]));
  const elements = all.map((el, i) => {
    const tag = el.tagName.toLowerCase();
    const rec = {
      i,
      parent: el === root ? null : index.get(el.parentElement) ?? null,
      tag,
      class: el.getAttribute("class") ?? "",
      text: (el.textContent ?? "").replace(/\s+/g, " ").trim().slice(0, 40),
      rect: rectOf(el.getBoundingClientRect(), R),
      style: styleOf(el),
    };
    const data = Object.fromEntries(
      [...el.attributes]
        .filter((a) => /^(data-|aria-|style$|type$|checked$|href$|id$)/.test(a.name) && a.name !== "data-id")
        .map((a) => [a.name, a.value]),
    );
    if (Object.keys(data).length) rec.attrs = data;
    const lines = el.getClientRects();
    if (getComputedStyle(el).display.startsWith("inline") && lines.length > 1) {
      rec.lineRects = [...lines].map((r) => rectOf(r, R));
    }
    if (tag === "li") {
      const cs = getComputedStyle(el);
      const glyph = firstGlyphRect(el, R);
      const marker = markerText(el);
      let markerEstimate = null;
      if (marker) {
        const ctx = document.createElement("canvas").getContext("2d");
        ctx.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
        const w = ctx.measureText(marker).width;
        const contentLeft = el.getBoundingClientRect().left + parseFloat(cs.paddingLeft) + parseFloat(cs.borderLeftWidth);
        markerEstimate = {
          text: marker,
          width: round(w),
          x: round(contentLeft - w - R.left),
          note: "canvas estimate: outside marker ends at the li content edge; see markerPixelBox for the measured glyph",
        };
      }
      rec.list = {
        listStyleType: cs.listStyleType,
        listStylePosition: cs.listStylePosition,
        contentX: round(el.getBoundingClientRect().left + parseFloat(cs.paddingLeft) - R.left),
        firstGlyph: glyph,
        markerEstimate,
      };
    }
    if (tag === "pre" || tag === "code" || tag === "p" || tag === "h1" || tag === "th" || tag === "span" && el.classList.contains("chat-markdown-codeblock-title")) {
      rec.usedFamily = usedFamily(el);
    }
    return rec;
  });

  const fonts = [...document.fonts].map((f) => ({
    family: f.family,
    weight: f.weight,
    style: f.style,
    status: f.status,
    unicodeRange: f.unicodeRange.length > 60 ? `${f.unicodeRange.slice(0, 60)}...` : f.unicodeRange,
  }));
  const scroller = document.querySelector("[data-reference-scroller]");
  const result = {
    page: location.pathname,
    theme: document.documentElement.classList.contains("dark") ? "dark" : "light",
    generatedAt: new Date().toISOString(),
    userAgent: navigator.userAgent,
    devicePixelRatio: devicePixelRatio,
    viewport: { width: innerWidth, height: innerHeight },
    rootPage: {
      x: round(R.left + scrollX + scroller.scrollLeft),
      y: round(R.top + scrollY + scroller.scrollTop),
      width: round(R.width),
      height: round(R.height),
    },
    documentHeight: Math.max(scroller.scrollHeight, document.documentElement.scrollHeight),
    fonts: {
      loaded: fonts.filter((f) => f.status === "loaded"),
      checks: {
        dmSans: document.fonts.check('14px "DM Sans Variable"'),
        jetbrainsMono400: document.fonts.check('400 12px "JetBrains Mono"'),
        jetbrainsMono500: document.fonts.check('500 12px "JetBrains Mono"'),
      },
      all: fonts,
    },
    elements,
  };
  const body = JSON.stringify(result, null, 1);
  const res = await fetch(`/${name}`, { method: "PUT", body });
  return { status: res.status, bytes: body.length, elements: elements.length, rootPage: result.rootPage, dpr: devicePixelRatio, loadedFonts: result.fonts.loaded.map((f) => `${f.family} ${f.weight}`) };
}

// Scroll helper for tile capture (?tile pages). Resolves after two frames.
export async function scrollToTile(x, y) {
  window.scrollTo(x, y);
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  return { x: scrollX, y: scrollY, w: innerWidth, h: innerHeight, docW: document.documentElement.scrollWidth, docH: document.documentElement.scrollHeight };
}
