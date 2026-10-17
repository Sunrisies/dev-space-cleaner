import type { Candidate, Kind } from "./types";
import { kinds } from "./types";
export type LanguageTotal = {
  count: number;
  bytes: number;
  pending: number;
  incomplete: number;
  removed: number;
};
export function summarize(
  items: Candidate[],
  removed: ReadonlySet<number>,
): Record<Kind, LanguageTotal> {
  const totals = Object.fromEntries(
    kinds.map((k) => [
      k,
      { count: 0, bytes: 0, pending: 0, incomplete: 0, removed: 0 },
    ]),
  ) as Record<Kind, LanguageTotal>;
  for (const item of items) {
    const total = totals[item.kind];
    if (removed.has(item.id)) {
      total.removed++;
      continue;
    }
    total.count++;
    total.bytes += item.bytes;
    if (!item.measured) total.pending++;
    else if (!item.complete) total.incomplete++;
  }
  return totals;
}
