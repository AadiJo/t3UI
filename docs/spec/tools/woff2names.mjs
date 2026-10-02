// Minimal WOFF2 reader: decompress the table stream and dump name + fvar + STAT summary.
import fs from "node:fs";
import zlib from "node:zlib";

const buf = fs.readFileSync(process.argv[2]);
let o = 0;
const u32 = () => { const v = buf.readUInt32BE(o); o += 4; return v; };
const u16 = () => { const v = buf.readUInt16BE(o); o += 2; return v; };
const u8 = () => buf[o++];
const base128 = () => { let v = 0; for (let i = 0; i < 5; i++) { const b = u8(); v = (v << 7) | (b & 0x7f); if (!(b & 0x80)) return v; } throw new Error("bad"); };
const KNOWN = ["cmap","head","hhea","hmtx","maxp","name","OS/2","post","cvt ","fpgm","glyf","loca","prep","CFF ","VORG","EBDT","EBLC","gasp","hdmx","kern","LTSH","PCLT","VDMX","vhea","vmtx","BASE","GDEF","GPOS","GSUB","EBSC","JSTF","MATH","CBDT","CBLC","COLR","CPAL","SVG ","sbix","acnt","avar","bdat","bloc","bsln","cvar","fdsc","feat","fmtx","fvar","gvar","hsty","just","lcar","mort","morx","opbd","prop","trak","Zapf","Silf","Glat","Gloc","Feat","Sill"];
u32(); u32(); u32(); const numTables = u16(); u16(); u32(); u16(); u16(); u32(); u32(); u32(); u32(); u32();
const totalCompressed = buf.readUInt32BE(20);
o = 48;
const tables = [];
for (let i = 0; i < numTables; i++) {
  const flags = u8();
  const tagIdx = flags & 0x3f;
  const tag = tagIdx === 63 ? buf.slice(o, (o += 4)).toString("latin1") : KNOWN[tagIdx];
  const xform = (flags >> 6) & 3;
  const origLength = base128();
  let length = origLength;
  const transformed = (tag === "glyf" || tag === "loca") ? xform === 0 : xform !== 0;
  if (transformed) length = base128();
  tables.push({ tag, length });
}
const data = zlib.brotliDecompressSync(buf.slice(o, o + totalCompressed));
let off = 0;
const t = {};
for (const tb of tables) { t[tb.tag] = data.slice(off, off + tb.length); off += tb.length; }

const name = t["name"];
const count = name.readUInt16BE(2), strOff = name.readUInt16BE(4);
const names = {};
for (let i = 0; i < count; i++) {
  const r = 6 + i * 12;
  const pid = name.readUInt16BE(r), nid = name.readUInt16BE(r + 6), len = name.readUInt16BE(r + 8), so = name.readUInt16BE(r + 10);
  if (pid !== 3) continue;
  const s = name.slice(strOff + so, strOff + so + len);
  let str = ""; for (let j = 0; j < s.length; j += 2) str += String.fromCharCode(s.readUInt16BE(j));
  names[nid] = str;
}
console.log("tables:", tables.map((x) => x.tag).join(","));
console.log("names:", JSON.stringify(names, null, 0));
if (t.fvar) {
  const f = t.fvar; const axesOff = f.readUInt16BE(4), axisCount = f.readUInt16BE(8), axisSize = f.readUInt16BE(10);
  for (let i = 0; i < axisCount; i++) {
    const a = axesOff + i * axisSize; const fx = (p) => f.readInt32BE(p) / 65536;
    console.log("axis", f.slice(a, a + 4).toString("latin1"), "min", fx(a + 4), "default", fx(a + 8), "max", fx(a + 12));
  }
}
if (t.STAT) {
  const s = t.STAT; const designAxisCount = s.readUInt16BE(6), daOff = s.readUInt32BE(8), avCount = s.readUInt16BE(12), avOff = s.readUInt32BE(14);
  const axes = []; for (let i = 0; i < designAxisCount; i++) axes.push(s.slice(daOff + i * 8, daOff + i * 8 + 4).toString("latin1"));
  console.log("STAT design axes:", axes.join(","));
  const vals = [];
  for (let i = 0; i < avCount; i++) {
    const p = avOff + s.readUInt16BE(avOff + i * 2); const fmt = s.readUInt16BE(p), axisIndex = s.readUInt16BE(p + 2), nameId = s.readUInt16BE(p + 6), value = s.readInt32BE(p + 8) / 65536;
    vals.push(`${axes[axisIndex]}=${value}(${names[nameId] ?? nameId}, fmt${fmt})`);
  }
  console.log("STAT values:", vals.join("; "));
}
