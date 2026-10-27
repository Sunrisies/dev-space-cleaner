import { invoke } from "@tauri-apps/api/core";
import type { Kind } from "./types";
import { kinds } from "./types";

export type Preferences = {
  version: 1;
  mode: "all" | "custom";
  roots: string[];
  kinds: Kind[];
  workers: number;
  global_mode?: boolean;
  global_ids?: string[];
  history_enabled?: boolean;
};
export const preferencesKey = "huanj-scan-settings-v1";

function validate(value: unknown): Preferences {
  const p = value as Partial<Preferences> | null;
  if (
    !p ||
    p.version !== 1 ||
    !["all", "custom"].includes(p.mode ?? "") ||
    !Array.isArray(p.roots) ||
    p.roots.some((r) => typeof r !== "string" || !r.trim()) ||
    !Array.isArray(p.kinds) ||
    !p.kinds.length ||
    p.kinds.some((k) => !kinds.includes(k)) ||
    !Number.isInteger(p.workers) ||
    p.workers! < 1 ||
    p.workers! > 16 ||
    (p.global_mode !== undefined && typeof p.global_mode !== "boolean") ||
    (p.history_enabled !== undefined &&
      typeof p.history_enabled !== "boolean") ||
    (p.global_ids !== undefined &&
      (!Array.isArray(p.global_ids) ||
        p.global_ids.length > 100 ||
        p.global_ids.some(
          (id) => typeof id !== "string" || !/^[a-zA-Z0-9-]{1,128}$/.test(id),
        ))) ||
    (p.global_mode === true && !p.global_ids?.length) ||
    (p.mode === "custom" && !p.roots.length && !p.global_mode)
  ) {
    throw new Error("保存的扫描设置格式不正确，请重新设置后保存");
  }
  return {
    ...(p as Preferences),
    global_mode: p.global_mode ?? false,
    global_ids: p.global_ids ?? [],
    history_enabled: p.history_enabled ?? false,
  };
}

export async function loadPreferences(
  native: boolean,
): Promise<Preferences | null> {
  if (native) {
    const value = await invoke<unknown>("load_preferences");
    return value === null ? null : validate(value);
  }
  // Browser preview has a separate store; desktop settings use the OS app config directory.
  const raw = localStorage.getItem(preferencesKey);
  if (raw === null) return null;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error("保存的扫描设置内容损坏，请重新设置后保存");
  }
  return validate(value);
}

export async function savePreferences(
  native: boolean,
  settings: Preferences,
): Promise<void> {
  validate(settings);
  if (native) {
    await invoke("save_preferences", { settings });
    return;
  }
  try {
    localStorage.setItem(preferencesKey, JSON.stringify(settings));
  } catch {
    throw new Error("无法保存扫描设置，请检查本机存储是否可用");
  }
}
