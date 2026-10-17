// Independent reference traversal. Only filesystem metadata is read.
// Usage: node scripts/audit-sizes.mjs <directory> [...directories]
import { lstat, readdir } from "node:fs/promises";
import { join } from "node:path";
import { performance } from "node:perf_hooks";
import { readFileSync } from "node:fs";
const args = process.argv.slice(2);
const expected = new Map();
let roots = args;
if (args[0] === "--scan") {
  const records = readFileSync(args[1], "utf8")
    .replace(/^\uFEFF/, "")
    .trim()
    .split(/\r?\n/)
    .map((line) => JSON.parse(line));
  roots = ["node", "rust", "python"]
    .map(
      (kind) =>
        records
          .filter(
            (e) =>
              e.type === "sized" && e.data.kind === kind && e.data.complete,
          )
          .map((e) => e.data)
          .sort((a, b) => b.bytes - a.bytes)[0],
    )
    .filter(Boolean)
    .map((item) => {
      expected.set(item.path, item);
      return item.path;
    });
}
for (const root of roots) {
  const start = performance.now();
  const stack = [root];
  let bytes = 0n,
    files = 0,
    skippedLinks = 0,
    directories = 0;
  const errors = [];
  while (stack.length) {
    const path = stack.pop();
    try {
      const meta = await lstat(path, { bigint: true });
      if (meta.isSymbolicLink()) {
        skippedLinks++;
        continue;
      }
      if (meta.isDirectory()) {
        directories++;
        for (const name of await readdir(path)) stack.push(join(path, name));
      } else if (meta.isFile()) {
        bytes += meta.size;
        files++;
      }
    } catch (e) {
      errors.push({ path, message: e.message });
    }
  }
  const previous = expected.get(root);
  const matches = previous
    ? bytes === BigInt(previous.bytes) &&
      files === previous.files &&
      skippedLinks === previous.skipped_links &&
      errors.length === previous.errors
    : null;
  if (matches === false) process.exitCode = 1;
  console.log(
    JSON.stringify({
      root,
      bytes: bytes.toString(),
      files,
      directories,
      skippedLinks,
      errors,
      elapsedMs: Math.round(performance.now() - start),
      matchesScan: matches,
    }),
  );
}
