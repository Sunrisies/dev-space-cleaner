import type { Candidate } from "./types";

export function cleanupImpact(item: Candidate): string {
  const rule = item.rule ?? "";
  if (rule.startsWith("global:")) return item.evidence;
  if (rule === "python-venv" || (!rule && item.kind === "python"))
    return "虚拟环境及其中的文件会一并移入回收站，之后需要重建环境和重新安装依赖。";
  if (
    ["node-modules", "node-npm", "composer-vendor", "bundler-vendor"].includes(
      rule,
    ) ||
    (!rule && item.kind === "node")
  )
    return "项目依赖会被移除，之后需要重新安装，可能需要联网。";
  if (
    [
      "python-bytecode",
      "python-tool-cache",
      "zig-cache",
      "gradle-project-cache",
    ].includes(rule)
  )
    return "工具缓存会被移除，后续运行时通常需要重新生成。";
  return "构建产物会被移除，之后需要重新编译；请确认没有手动保存的文件。";
}
