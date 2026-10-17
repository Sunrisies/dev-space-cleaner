import { readFileSync } from "node:fs";
function load(path) {
  const lines = readFileSync(path, "utf8")
    .replace(/^\uFEFF/, "")
    .trim()
    .split(/\r?\n/)
    .map((line) => JSON.parse(line));
  const items = lines.filter((e) => e.type === "sized").map((e) => e.data);
  return { summary: lines.findLast((e) => e.type === "finished")?.data, items };
}
const before = load(process.argv[2]);
console.log("Before:", JSON.stringify(before.summary));
if (process.argv[3]) {
  const after = load(process.argv[3]);
  console.log("After:", JSON.stringify(after.summary));
  const previous = new Map(before.items.map((i) => [i.path, i]));
  let mismatched = 0;
  for (const item of after.items) {
    const old = previous.get(item.path);
    if (
      !old ||
      ["kind", "bytes", "files", "complete", "errors", "skipped_links"].some(
        (k) => old[k] !== item[k],
      )
    )
      mismatched++;
    previous.delete(item.path);
  }
  console.log(
    JSON.stringify({
      differentResults: mismatched,
      missingResults: previous.size,
    }),
  );
}
