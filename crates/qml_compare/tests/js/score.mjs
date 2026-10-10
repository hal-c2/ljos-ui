// Scores picture pairs with the web gallery's own compare.js, unmodified.
// usage: node score.mjs <gallery dir> <actual.png> <reference.png> [...pairs]
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const [gallery, ...files] = process.argv.slice(2);
// compare.js resolves pixelmatch and pngjs from its own node_modules.
const { compare } = await import(pathToFileURL(`${gallery}/tests/compare.js`).href);
const out = [];
for (let i = 0; i < files.length; i += 2) {
  const { diff, ...scores } = compare(readFileSync(files[i]), readFileSync(files[i + 1]));
  out.push({ ...scores, diffBase64: diff ? diff.toString("base64") : null });
}
console.log(JSON.stringify(out));
