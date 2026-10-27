import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { displayPath, kindInfo, size, type CleanupRecord } from "./types";

export function CleanupHistory({ records }: { records: CleanupRecord[] }) {
  const [query, setQuery] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
  }, [query]);
  const filtered = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return records.filter(
      (record) =>
        !needle ||
        `${record.path} ${record.message} ${kindInfo[record.kind]?.label ?? ""}`
          .toLocaleLowerCase()
          .includes(needle),
    );
  }, [records, query]);
  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 104,
    overscan: 5,
  });
  return (
    <>
      <label className="search cleanup-history-search">
        <input
          aria-label="搜索清理记录"
          placeholder="搜索路径或错误原因…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>
      <div
        className="cleanup-history-list"
        ref={scrollRef}
        aria-label="清理记录列表"
      >
        {filtered.length === 0 ? (
          <p className="small muted">
            {query ? "没有匹配的清理记录。" : "还没有清理记录。"}
          </p>
        ) : (
          <div
            style={{ height: virtualizer.getTotalSize(), position: "relative" }}
          >
            {virtualizer.getVirtualItems().map((row) => {
              const record = filtered[row.index];
              return (
                <article
                  className="cleanup-history-row"
                  key={row.key}
                  data-index={row.index}
                  ref={virtualizer.measureElement}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translateY(${row.start}px)`,
                  }}
                >
                  <div>
                    <strong
                      className={record.success ? "success-text" : "error-text"}
                    >
                      {record.success ? "已移入回收站" : "清理失败"}
                    </strong>
                    <span>
                      {kindInfo[record.kind]?.label ?? record.kind} ·{" "}
                      {size(record.bytes)}
                    </span>
                    <time>
                      {new Date(record.timestamp_ms).toLocaleString()}
                    </time>
                  </div>
                  <code>{displayPath(record.path)}</code>
                  <p>{record.message}</p>
                </article>
              );
            })}
          </div>
        )}
      </div>
      <p className="small muted">
        仅保存在本机，最多保留最近 1000
        项。能否恢复取决于系统回收站是否仍保留文件。
      </p>
    </>
  );
}
