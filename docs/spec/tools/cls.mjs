// Usage: node cls.mjs 'class-a' 'class-b' ...  -> prints compiled rule(s) from out.css
import fs from "node:fs";
// Scratch dir for generated intermediates (compiled CSS, colors.json). Override with T3SPEC_TMP.
const TMP = process.env.T3SPEC_TMP ?? "/tmp/t3spec";
fs.mkdirSync(TMP, { recursive: true });
const css = fs.readFileSync(`${TMP}/out.css`, "utf8");
const esc = (s) => s.replace(/[^a-zA-Z0-9_-]/g, (c) => "\\" + c).replace(/^(\d)/, "\\3$1 ");
for (const name of process.argv.slice(2)) {
  const needle = "." + esc(name);
  let idx = 0;
  let found = false;
  while ((idx = css.indexOf(needle, idx)) !== -1) {
    const after = css[idx + needle.length];
    if (after !== " " && after !== "{" && after !== ":" && after !== "," && after !== ")") { idx++; continue; }
    // walk back to line start
    const lineStart = css.lastIndexOf("\n", idx) + 1;
    // find opening brace and matching close
    const open = css.indexOf("{", idx);
    let depth = 0, i = open;
    for (; i < css.length; i++) {
      if (css[i] === "{") depth++;
      else if (css[i] === "}") { depth--; if (depth === 0) break; }
    }
    console.log(css.slice(lineStart, i + 1));
    found = true;
    idx = i;
    break;
  }
  if (!found) console.log(`/* NOT FOUND: ${name} */`);
}
