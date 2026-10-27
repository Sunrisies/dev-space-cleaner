import { invoke } from "@tauri-apps/api/core";
import { kindInfo, type Candidate, type Kind, type Summary } from "./types";

export type ScanHistory = {
  version: 1;
  saved_at: number;
  items: Candidate[];
  summary: Summary;
  scan_kinds: Kind[];
};
const key = "huanj-scan-history-v1";
const number = (v: unknown) =>
  typeof v === "number" && Number.isSafeInteger(v) && v >= 0;
function validate(value: unknown): ScanHistory {
  const h = value as Partial<ScanHistory> | null;
  if (
    !h ||
    h.version !== 1 ||
    !number(h.saved_at) ||
    !Array.isArray(h.items) ||
    h.items.length > 100_000 ||
    !Array.isArray(h.scan_kinds) ||
    h.scan_kinds.some((k) => !Object.hasOwn(kindInfo, k)) ||
    !h.summary ||
    typeof h.summary.cancelled !== "boolean" ||
    !number(h.summary.elapsed_ms) ||
    !Array.isArray(h.summary.roots) ||
    h.summary.roots.some((r) => typeof r !== "string") ||
    !h.summary.progress ||
    ["directories", "files", "found", "bytes", "errors", "skipped_links"].some(
      (k) =>
        !number((h.summary!.progress as unknown as Record<string, unknown>)[k]),
    )
  )
    throw new Error("历史扫描记录格式无效，请重新扫描");
  const ids = new Set<number>();
  for (const item of h.items) {
    if (
      !item ||
      !number(item.id) ||
      item.id === 0 ||
      ids.has(item.id) ||
      typeof item.path !== "string" ||
      !/^([a-z]:[\\/]|\\\\|\/)/i.test(item.path) ||
      typeof item.root !== "string" ||
      typeof item.project !== "string" ||
      !Object.hasOwn(kindInfo, item.kind) ||
      typeof item.evidence !== "string" ||
      (item.rule !== undefined && typeof item.rule !== "string") ||
      !number(item.bytes) ||
      !number(item.files) ||
      !number(item.skipped_links) ||
      !number(item.errors) ||
      typeof item.complete !== "boolean" ||
      typeof item.measured !== "boolean" ||
      (item.modified !== null && !number(item.modified))
    )
      throw new Error("历史扫描记录含有无效目录，请重新扫描");
    ids.add(item.id);
  }
  return h as ScanHistory;
}
export async function loadHistory(
  native: boolean,
): Promise<ScanHistory | null> {
  const value = native
    ? await invoke<unknown>("load_scan_history")
    : (() => {
        const raw = localStorage.getItem(key);
        return raw === null ? null : JSON.parse(raw);
      })();
  return value === null ? null : validate(value);
}
export async function saveHistory(
  native: boolean,
  snapshot: ScanHistory,
): Promise<void> {
  validate(snapshot);
  if (native) await invoke("save_scan_history", { snapshot });
  else localStorage.setItem(key, JSON.stringify(snapshot));
}
