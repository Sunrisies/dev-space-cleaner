import { useState } from "react";
import { Search } from "lucide-react";
import { projectKinds as kinds, kindInfo, type Kind } from "./types";

export function ToolchainPicker({
  selected,
  onChange,
  disabled,
}: {
  selected: ReadonlySet<Kind>;
  onChange: (value: Set<Kind>) => void;
  disabled: boolean;
}) {
  const [query, setQuery] = useState("");
  const needle = query.trim().toLowerCase();
  const visible = kinds.filter((k) =>
    `${k} ${kindInfo[k].label} ${kindInfo[k].hint} ${kindInfo[k].dir}`
      .toLowerCase()
      .includes(needle),
  );
  return (
    <section className="toolchain-picker" aria-label="项目扫描类型">
      <div className="toolchain-heading">
        <h3>
          项目扫描类型{" "}
          <small>
            {selected.size} / {kinds.length}
          </small>
        </h3>
        <button
          type="button"
          className="text-button"
          disabled={disabled}
          onClick={() => {
            const next = new Set(selected);
            const clear =
              visible.length > 0 && visible.every((k) => next.has(k));
            for (const k of visible) {
              if (clear) next.delete(k);
              else next.add(k);
            }
            onChange(next);
          }}
        >
          {visible.length > 0 && visible.every((k) => selected.has(k))
            ? "取消当前筛选"
            : "选择当前筛选"}
        </button>
      </div>
      <label className="search type-search">
        <Search size={15} />
        <input
          aria-label="搜索语言或工具链"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索 Android、Java、CMake…"
        />
      </label>
      <div className="toolchain-list">
        {visible.map((k) => (
          <label
            key={k}
            className={`toolchain-option ${selected.has(k) ? "chosen" : ""}`}
          >
            <input
              type="checkbox"
              aria-label={kindInfo[k].label}
              disabled={disabled}
              checked={selected.has(k)}
              onChange={() => {
                const next = new Set(selected);
                if (next.has(k)) next.delete(k);
                else next.add(k);
                onChange(next);
              }}
            />
            <span className={`lang-icon ${k}`}>{kindInfo[k].letter}</span>
            <span>
              <strong>{kindInfo[k].label}</strong>
              <small>{kindInfo[k].hint}</small>
            </span>
          </label>
        ))}
        {!visible.length && <p className="small muted">没有匹配的项目类型。</p>}
      </div>
      <p className="small muted">
        仅扫描有项目与产物标识的目录；全局缓存可在扫描范围中单独选择。
      </p>
    </section>
  );
}
