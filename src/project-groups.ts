import { displayPath, type Candidate } from "./types";

export type ProjectGroup = {
  key: string;
  path: string;
  name: string;
  items: Candidate[];
  bytes: number;
  incomplete: boolean;
  pending: boolean;
};

function parent(path: string): string {
  const value = displayPath(path).replace(/[\\/]+$/, "");
  const i = Math.max(value.lastIndexOf("/"), value.lastIndexOf("\\"));
  if (i < 0) return value;
  if (i === 0) return value.slice(0, 1);
  if (i === 2 && value[1] === ":") return value.slice(0, 3);
  return value.slice(0, i);
}

export function volumeOf(path: string): string {
  const value = displayPath(path).replace(/\\/g, "/");
  const drive = /^([A-Za-z]):\//.exec(value);
  if (drive) return `${drive[1].toUpperCase()}:`;
  const unc = /^\/\/([^/]+)\/([^/]+)/.exec(value);
  if (unc) return `//${unc[1]}/${unc[2]}`;
  const volume = /^\/Volumes\/([^/]+)/.exec(value);
  return volume ? `/Volumes/${volume[1]}` : "/";
}

export function groupPath(item: Candidate): string {
  if (item.rule?.startsWith("global:")) return displayPath(item.path);
  let path = parent(item.path);
  if (item.rule === "bundler-vendor") path = parent(path);
  return path;
}

export function groupProjects(
  items: Candidate[],
  removed: ReadonlySet<number>,
): ProjectGroup[] {
  const groups = new Map<string, ProjectGroup>();
  for (const item of items) {
    const path = groupPath(item);
    const key = /^[A-Za-z]:[\\/]|^\\\\/.test(path)
      ? path.toLocaleLowerCase()
      : path;
    let group = groups.get(key);
    if (!group) {
      const name = item.rule?.startsWith("global:")
        ? item.project
        : path
            .replace(/[\\/]+$/, "")
            .split(/[\\/]/)
            .pop() || item.project;
      group = {
        key,
        path,
        name,
        items: [],
        bytes: 0,
        incomplete: false,
        pending: false,
      };
      groups.set(key, group);
    }
    group.items.push(item);
    if (!removed.has(item.id)) {
      group.bytes += item.bytes;
      group.pending ||= !item.measured;
      group.incomplete ||= item.measured && !item.complete;
    }
  }
  return [...groups.values()];
}
