// Compare scan results without printing personal project paths.
import { readFileSync } from "node:fs";
function load(file) {
  const events = readFileSync(file, "utf8")
    .replace(/^\uFEFF/, "")
    .trim()
    .split(/\r?\n/)
    .map(JSON.parse);
  const summary = events.findLast((e) => e.type === "finished")?.data;
  if (!summary || summary.cancelled)
    throw Error("Comparison requires two completed scans");
  return {
    items: events.filter((e) => e.type === "sized").map((e) => e.data),
    summary,
  };
}
const before = load(process.argv[2]);
const after = load(process.argv[3]);
const current = new Map(after.items.map((i) => [i.path, i]));
let identical = 0,
  changed = 0,
  covered = 0,
  missing = 0;
for (const old of before.items) {
  const item = current.get(old.path);
  if (item) {
    if (
      ["kind", "bytes", "files", "complete", "errors", "skipped_links"].every(
        (k) => old[k] === item[k],
      )
    )
      identical++;
    else changed++;
  } else if (
    after.items.some(
      (i) =>
        old.path.startsWith(i.path + "\\") || old.path.startsWith(i.path + "/"),
    )
  )
    covered++;
  else missing++;
}
const totals = {};
for (const item of after.items) {
  const total = (totals[item.kind] ??= { count: 0, bytes: 0 });
  total.count++;
  total.bytes += item.bytes;
}
let audit;
if (process.argv[4]) {
  const samples = readFileSync(process.argv[4], "utf8")
    .replace(/^\uFEFF/, "")
    .trim()
    .split(/\r?\n/)
    .map(JSON.parse);
  audit = {
    samples: samples.length,
    matching: samples.filter((s) => s.matchesScan === true).length,
    errors: samples.reduce((sum, s) => sum + s.errors.length, 0),
  };
  if (audit.samples === 0 || audit.matching !== audit.samples || audit.errors)
    process.exitCode = 1;
}
console.log(
  JSON.stringify(
    {
      before: before.summary,
      after: after.summary,
      identical,
      changed,
      coveredByParent: covered,
      missing,
      totals,
      audit,
    },
    null,
    2,
  ),
);
