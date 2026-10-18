// npm test: one runnable check for the transcript sentence rule (src/lib/api.ts): mid-cue full stops split, pauses split, short cues merge.
import { readFileSync } from "node:fs";
const src = readFileSync(new URL("../src/lib/api.ts", import.meta.url), "utf8");
const body = src.slice(src.indexOf("export function sentences"), src.indexOf("/** m:ss.s"))
  .replace("export function sentences(cues: Cue[]): Cue[]", "function sentences(cues)")
  .replace(/: \{ a: number; b: number; t: string; end: boolean \}\[\]/, "").replace(/: Cue\[\]/g, "").replace(/: Cue \| null/, "");
const sentences = new Function(body + "; return sentences;")();
const rows = sentences([[0, 2, "Hi team, this is"], [2, 4, "a demo. Next we"], [4, 6, "open billing"], [9, 10, "Thanks."]]);
const assert = (c, m) => { if (!c) { console.error("FAIL", m, JSON.stringify(rows)); process.exit(1); } };
assert(rows.length === 3, "three rows");
assert(rows[0][2] === "Hi team, this is a demo.", "first sentence");
assert(rows[1][0] > 2 && rows[1][0] < 4, "mid-cue start estimated");
assert(rows[2][2] === "Thanks." && rows[2][0] === 9, "pause splits");
console.log("sentences ok", JSON.stringify(rows.map((r) => [Math.round(r[0] * 10) / 10, r[2]])));
