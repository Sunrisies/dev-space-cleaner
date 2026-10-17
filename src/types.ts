import catalog from "./toolchains.json" with { type: "json" };
export type Kind = keyof typeof catalog;
export const kinds = Object.keys(catalog) as Kind[];
export const kindInfo = catalog;
export type Drive = { path: string; label: string };
export type Candidate = {
  id: number;
  path: string;
  root: string;
  project: string;
  kind: Kind;
  rule?: string;
  evidence: string;
  bytes: number;
  files: number;
  complete: boolean;
  measured: boolean;
  skipped_links: number;
  errors: number;
  modified: number | null;
};
export type Progress = {
  directories: number;
  files: number;
  found: number;
  bytes: number;
  errors: number;
  skipped_links: number;
};
export type Summary = {
  progress: Progress;
  cancelled: boolean;
  elapsed_ms: number;
  roots: string[];
};
export type ScanEvent =
  | { type: "found" | "sized"; data: Candidate }
  | { type: "progress"; data: { progress: Progress; path: string } }
  | { type: "issue"; data: { path: string; message: string } }
  | { type: "root_started"; data: { path: string } }
  | {
      type: "root_finished";
      data: { path: string; errors: number; cancelled: boolean };
    }
  | { type: "root_skipped"; data: { path: string; reason: string } }
  | { type: "finished"; data: Summary };
export type Envelope = { session: string; event: ScanEvent };
export type CleanResult = { id: number; success: boolean; message: string };
export const emptyProgress: Progress = {
  directories: 0,
  files: 0,
  found: 0,
  bytes: 0,
  errors: 0,
  skipped_links: 0,
};
export function directoryName(item: Candidate): string {
  return (
    displayPath(item.path)
      .replace(/[\\/]+$/, "")
      .split(/[\\/]/)
      .pop() || item.path
  );
}
export function size(bytes: number): string {
  if (!bytes) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  const i = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  return `${(bytes / 1024 ** i).toLocaleString("en-US", { maximumFractionDigits: i > 0 ? 2 : 0 })} ${units[i]}`;
}
export function displayPath(path: string): string {
  return path.startsWith("\\\\?\\UNC\\")
    ? "\\\\" + path.slice(8)
    : path.replace(/^\\\\\?\\/, "");
}
