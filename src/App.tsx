import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { invoke, isTauri, Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useVirtualizer } from "@tanstack/react-virtual";
import { summarize } from "./summary";
import {
  loadPreferences,
  savePreferences,
  type Preferences,
} from "./preferences";
import {
  ArrowDownWideNarrow,
  ArrowUpRight,
  Check,
  CheckCheck,
  ChevronRight,
  CircleHelp,
  Clock3,
  Code2,
  Cpu,
  Folder,
  FolderOpen,
  HardDrive,
  Layers3,
  LoaderCircle,
  Monitor,
  Moon,
  Play,
  Plus,
  Search,
  Settings2,
  ShieldCheck,
  Sparkles,
  Square,
  Sun,
  Trash2,
  X,
} from "lucide-react";
import {
  displayPath,
  emptyProgress,
  kindInfo,
  size,
  type Candidate,
  type CleanResult,
  type Drive,
  type Envelope,
  type Kind,
  type Progress,
  type Summary,
} from "./types";

const kinds: Kind[] = ["node", "rust", "python"];
type Theme = "system" | "light" | "dark";
type RootStatus = {
  phase: "scanning" | "done" | "cancelled" | "skipped";
  errors?: number;
  reason?: string;
};

function Modal({
  title,
  children,
  onClose,
  wide = false,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  wide?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    ref.current?.showModal();
  }, []);
  return (
    <dialog
      ref={ref}
      className={wide ? "modal wide" : "modal"}
      onCancel={onClose}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <header>
        <h2>{title}</h2>
        <button className="icon-button" onClick={onClose} aria-label="关闭">
          <X size={19} />
        </button>
      </header>
      {children}
    </dialog>
  );
}

export default function App() {
  const native = isTauri();
  const [drives, setDrives] = useState<Drive[]>([]);
  const [mode, setMode] = useState<"all" | "custom">("all");
  const [customRoots, setCustomRoots] = useState<string[]>([]);
  const [enabledKinds, setEnabledKinds] = useState<Set<Kind>>(new Set(kinds));
  const [filter, setFilter] = useState<Kind | "all">("all");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<"size" | "name">("size");
  const [items, setItems] = useState<Map<number, Candidate>>(new Map());
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [progress, setProgress] = useState<Progress>(emptyProgress);
  const [scanning, setScanning] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [currentPath, setCurrentPath] = useState("");
  const [rootStatuses, setRootStatuses] = useState<Record<string, RootStatus>>(
    {},
  );
  const [issues, setIssues] = useState<{ path: string; message: string }[]>([]);
  const [message, setMessage] = useState("");
  const [settings, setSettings] = useState(false);
  const [showIssues, setShowIssues] = useState(false);
  const [help, setHelp] = useState(false);
  const [detailSnapshot, setDetail] = useState<Candidate | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [cleanReport, setCleanReport] = useState(false);
  const [cleanResults, setCleanResults] = useState<Map<number, CleanResult>>(
    new Map(),
  );
  const [theme, setTheme] = useState<Theme>(() => {
    try {
      return (localStorage.getItem("huanj-theme") as Theme) || "system";
    } catch {
      return "system";
    }
  });
  const [workers, setWorkers] = useState(8);
  const [contextMenu, setContextMenu] = useState<{
    id: number;
    x: number;
    y: number;
  } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [preferencesReady, setPreferencesReady] = useState(false);
  const [savingPreferences, setSavingPreferences] = useState(false);
  const [preferencesError, setPreferencesError] = useState("");
  const [savedPreferences, setSavedPreferences] = useState<Preferences | null>(
    null,
  );
  const session = useRef("");
  const staged = useRef(new Map<number, Candidate>());
  const busy = scanning || cleaning || !preferencesReady || savingPreferences;
  const scrollRef = useRef<HTMLDivElement>(null);
  const currentPreferences = useMemo<Preferences>(
    () => ({
      version: 1,
      mode,
      roots: customRoots,
      kinds: kinds.filter((k) => enabledKinds.has(k)),
      workers,
    }),
    [mode, customRoots, enabledKinds, workers],
  );
  const preferencesSaved =
    savedPreferences !== null &&
    JSON.stringify(currentPreferences) === JSON.stringify(savedPreferences);

  useEffect(() => {
    let live = true;
    loadPreferences(native)
      .then((saved) => {
        if (!live) return;
        if (saved) {
          setMode(saved.mode);
          setCustomRoots(saved.roots);
          setEnabledKinds(new Set(saved.kinds));
          setWorkers(saved.workers);
          setSavedPreferences(saved);
        }
      })
      .catch((error) => {
        if (live)
          setMessage(
            `读取扫描设置失败：${String(error)}。当前使用默认设置，原配置未改动。`,
          );
      })
      .finally(() => {
        if (live) setPreferencesReady(true);
      });
    return () => {
      live = false;
    };
  }, [native]);

  useEffect(() => {
    if (settings) setPreferencesError("");
  }, [settings]);

  async function rememberSettings() {
    if (busy) return;
    setSavingPreferences(true);
    setPreferencesError("");
    try {
      await savePreferences(native, currentPreferences);
      setSavedPreferences(currentPreferences);
      setSettings(false);
    } catch (error) {
      setPreferencesError(String(error));
    } finally {
      setSavingPreferences(false);
    }
  }

  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme =
        theme === "system" ? (media.matches ? "dark" : "light") : theme;
    };
    apply();
    media.addEventListener("change", apply);
    try {
      localStorage.setItem("huanj-theme", theme);
    } catch {
      /* Theme still works without persistence. */
    }
    return () => media.removeEventListener("change", apply);
  }, [theme]);

  useEffect(() => {
    if (native)
      invoke<Drive[]>("list_drives")
        .then(setDrives)
        .catch((e) => setMessage(String(e)));
  }, [native]);
  const flush = useCallback(() => {
    if (!staged.current.size) return;
    const batch = new Map(staged.current);
    staged.current.clear();
    setItems((old) => {
      const next = new Map(old);
      for (const [id, item] of batch) next.set(id, item);
      return next;
    });
  }, []);
  useEffect(() => {
    if (!scanning) return;
    const timer = setInterval(flush, 150);
    return () => clearInterval(timer);
  }, [scanning, flush]);

  const allItems = useMemo(() => Array.from(items.values()), [items]);
  const failedItems = allItems.filter(
    (item) => cleanResults.get(item.id)?.success === false,
  );
  const removed = useMemo(
    () =>
      new Set(
        [...cleanResults.values()].filter((r) => r.success).map((r) => r.id),
      ),
    [cleanResults],
  );
  const contextItem = contextMenu ? items.get(contextMenu.id) : null;
  const detail =
    detailSnapshot && (items.get(detailSnapshot.id) ?? detailSnapshot);
  const visible = useMemo(() => {
    const q = query.trim().toLocaleLowerCase();
    return allItems
      .filter(
        (i) =>
          (filter === "all" || i.kind === filter) &&
          (!q ||
            i.path.toLocaleLowerCase().includes(q) ||
            i.project.toLocaleLowerCase().includes(q)),
      )
      .sort((a, b) =>
        sort === "size"
          ? b.bytes - a.bytes || a.id - b.id
          : a.project.localeCompare(b.project),
      );
  }, [allItems, filter, query, sort]);
  const selectable = (i: Candidate) =>
    i.measured && i.complete && !cleanResults.get(i.id)?.success;
  const chosen = allItems.filter((i) => selected.has(i.id) && selectable(i));
  const chosenBytes = chosen.reduce((s, i) => s + i.bytes, 0);
  const eligible = visible.filter(selectable);
  const allChecked =
    eligible.length > 0 && eligible.every((i) => selected.has(i.id));
  const virtualizer = useVirtualizer({
    count: visible.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 64,
    overscan: 7,
    getItemKey: (i) => visible[i].id,
  });
  const totals = useMemo(
    () => summarize(allItems, removed),
    [allItems, removed],
  );
  const remainingBytes = kinds.reduce((sum, k) => sum + totals[k].bytes, 0);
  const remainingCount = kinds.reduce((sum, k) => sum + totals[k].count, 0);
  const totalText = (kind: Kind) => {
    const total = totals[kind];
    if (!summary && !scanning && !allItems.length) return "等待扫描";
    const label =
      scanning || total.pending
        ? " · 统计中"
        : failedItems.some((item) => item.kind === kind)
          ? " · 待复核"
          : total.incomplete || summary?.cancelled || progress.errors
            ? " · 部分统计"
            : "";
    return size(total.bytes) + label;
  };

  function showContextMenu(id: number, x: number, y: number) {
    setContextMenu({
      id,
      x: Math.max(8, Math.min(x, innerWidth - 240)),
      y: Math.max(8, Math.min(y, innerHeight - 120)),
    });
  }
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    const initialScroll = scrollRef.current?.scrollTop;
    const scroll = () => {
      if (scrollRef.current?.scrollTop !== initialScroll) close();
    };
    const pointer = (e: PointerEvent) => {
      if (!menuRef.current?.contains(e.target as Node)) close();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
        document
          .querySelector<HTMLElement>(`[data-candidate-id="${contextMenu.id}"]`)
          ?.focus();
      }
    };
    document.addEventListener("pointerdown", pointer);
    document.addEventListener("keydown", key);
    window.addEventListener("scroll", scroll, true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("pointerdown", pointer);
      document.removeEventListener("keydown", key);
      window.removeEventListener("scroll", scroll, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, [contextMenu]);

  async function openDirectory(item: Candidate) {
    setContextMenu(null);
    if (!native) {
      setMessage("请在桌面应用中打开本机目录。");
      return;
    }
    try {
      await invoke("open_candidate_directory", {
        session: session.current,
        id: item.id,
      });
    } catch (error) {
      setMessage(String(error));
    }
  }
  const rootList = mode === "all" ? drives.map((d) => d.path) : customRoots;
  const finishedRoots = Object.values(rootStatuses).filter(
    (s) => s.phase === "done" || s.phase === "skipped",
  ).length;
  const toggleSelected = (id: number) =>
    setSelected((old) => {
      const next = new Set(old);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const toggleAll = () =>
    setSelected((old) => {
      const next = new Set(old);
      for (const item of eligible) {
        if (allChecked) next.delete(item.id);
        else next.add(item.id);
      }
      return next;
    });

  async function start() {
    setContextMenu(null);
    if (!native) {
      setMessage("请通过桌面应用运行以访问本机磁盘。浏览器仅用于查看界面。");
      return;
    }
    if (mode === "custom" && !customRoots.length) {
      setSettings(true);
      return;
    }
    const id = crypto.randomUUID();
    session.current = id;
    staged.current.clear();
    setItems(new Map());
    setSelected(new Set());
    setCleanResults(new Map());
    setCleanReport(false);
    setProgress(emptyProgress);
    setSummary(null);
    setIssues([]);
    setRootStatuses({});
    setCurrentPath("正在准备磁盘…");
    setMessage("");
    setScanning(true);
    setCancelling(false);
    const channel = new Channel<Envelope>();
    channel.onmessage = ({ session: incoming, event }) => {
      if (incoming !== session.current) return;
      switch (event.type) {
        case "found":
        case "sized":
          staged.current.set(event.data.id, event.data);
          break;
        case "progress":
          setProgress(event.data.progress);
          setCurrentPath(displayPath(event.data.path));
          break;
        case "issue":
          setIssues((old) => [...old, event.data].slice(0, 100));
          break;
        case "root_started":
          setRootStatuses((old) => ({
            ...old,
            [displayPath(event.data.path)]: { phase: "scanning" },
          }));
          break;
        case "root_finished":
          setRootStatuses((old) => ({
            ...old,
            [displayPath(event.data.path)]: {
              phase: event.data.cancelled ? "cancelled" : "done",
              errors: event.data.errors,
            },
          }));
          break;
        case "root_skipped":
          setRootStatuses((old) => ({
            ...old,
            [displayPath(event.data.path)]: {
              phase: "skipped",
              reason: event.data.reason,
            },
          }));
          break;
        case "finished":
          flush();
          setSummary(event.data);
          setProgress(event.data.progress);
          setScanning(false);
          setCancelling(false);
          setCurrentPath("");
          break;
      }
    };
    try {
      await invoke("start_scan", {
        session: id,
        roots: mode === "all" ? null : customRoots,
        kinds: Array.from(enabledKinds),
        workers,
        onEvent: channel,
      });
    } catch (error) {
      setMessage(String(error));
      setScanning(false);
    }
  }
  async function cancel() {
    setCancelling(true);
    try {
      await invoke("cancel_scan", { session: session.current });
    } catch (error) {
      setMessage(String(error));
      setCancelling(false);
    }
  }
  async function cleanSelected() {
    const ids = chosen.map((i) => i.id);
    setConfirm(false);
    setCleaning(true);
    setMessage("");
    try {
      const results = await invoke<CleanResult[]>("clean_selected", {
        session: session.current,
        ids,
      });
      setCleanResults(
        (old) => new Map([...old, ...results.map((r) => [r.id, r] as const)]),
      );
      const success = new Set(
        results.filter((r) => r.success).map((r) => r.id),
      );
      if (results.some((result) => !result.success)) setCleanReport(true);
      setSelected((old) => new Set([...old].filter((id) => !success.has(id))));
      setMessage(
        `已移入回收站 ${success.size} 项${results.length > success.size ? `，${results.length - success.size} 项失败，请查看对应目录详情` : "。清空回收站后才会释放空间。"}`,
      );
    } catch (error) {
      setMessage(String(error));
    } finally {
      setCleaning(false);
    }
  }
  async function addFolder() {
    if (!native) {
      setMessage("选择本机目录需要在桌面应用中操作。");
      return;
    }
    try {
      const paths = await open({
        directory: true,
        multiple: true,
        title: "选择要扫描的目录",
      });
      if (paths) {
        setCustomRoots((old) => [...new Set([...old, ...paths])]);
        setMode("custom");
      }
    } catch (error) {
      setMessage(String(error));
    }
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">
            <Layers3 size={23} strokeWidth={1.6} />
          </div>
          <div>
            <strong>
              缓境<span>HUANJ</span>
            </strong>
            <small>给开发留一点空间</small>
          </div>
        </div>
        <div className="nav-label">工作空间</div>
        <button
          className={`nav-item ${filter === "all" ? "active" : ""}`}
          onClick={() => setFilter("all")}
        >
          <HardDrive size={18} />
          <span>空间扫描</span>
          <ChevronRight size={15} />
        </button>
        <div className="nav-label languages-label">
          开发环境<span>逻辑体积汇总</span>
        </div>
        {kinds.map((k) => (
          <button
            key={k}
            className={`nav-item language-nav ${filter === k ? "active" : ""}`}
            aria-label={`${kindInfo[k].label}：${totals[k].count} 个剩余目录，${totalText(k)}`}
            onClick={() => setFilter(filter === k ? "all" : k)}
          >
            <span className={`lang-icon ${k}`}>{kindInfo[k].letter}</span>
            <span className="language-copy">
              <span>{kindInfo[k].label}</span>
              <small>{totalText(k)}</small>
            </span>
            <span className="nav-count">{totals[k].count}</span>
          </button>
        ))}
        <div className="sidebar-bottom">
          <div className="local-note">
            <ShieldCheck size={18} />
            <div>
              <strong>只在你的电脑上运行</strong>
              <p>扫描与清理均在本地完成</p>
            </div>
          </div>
          <button className="nav-item" onClick={() => setSettings(true)}>
            <Settings2 size={17} />
            <span>扫描设置</span>
          </button>
          <button className="nav-item" onClick={() => setHelp(true)}>
            <CircleHelp size={17} />
            <span>使用说明</span>
            <span className="version">v0.1.5</span>
          </button>
        </div>
      </aside>

      <main className="main">
        <header className="topbar">
          <div className="breadcrumb">
            工作空间
            <ChevronRight size={13} />
            <h1 className="page-title">空间扫描</h1>
          </div>
          <div className="top-actions">
            <span className="local-badge">
              <i />
              本地运行
            </span>
            <button
              className="icon-button theme-button"
              title={`主题：${theme === "system" ? "跟随系统" : theme === "dark" ? "深色" : "浅色"}`}
              aria-label="切换明暗主题"
              onClick={() =>
                setTheme(
                  theme === "system"
                    ? "light"
                    : theme === "light"
                      ? "dark"
                      : "system",
                )
              }
            >
              {theme === "system" ? (
                <Monitor size={17} />
              ) : theme === "dark" ? (
                <Moon size={17} />
              ) : (
                <Sun size={17} />
              )}
            </button>
          </div>
        </header>
        <section className="workspace">
          {message && (
            <div className="notice" role="status">
              <CircleHelp size={16} />
              <span>{message}</span>
              {failedItems.length > 0 && (
                <button
                  className="text-button"
                  onClick={() => setCleanReport(true)}
                >
                  查看失败目录（{failedItems.length}）
                </button>
              )}
              <button
                className="icon-button"
                aria-label="关闭提示"
                onClick={() => setMessage("")}
              >
                <X size={15} />
              </button>
            </div>
          )}
          {!native && (
            <div className="preview-note">
              <Monitor size={14} />
              界面预览模式 · 实际扫描与清理请运行桌面应用
            </div>
          )}

          <section className="scan-panel" aria-label="扫描控制">
            <div className="scan-controls">
              <div className="scope-icon">
                <HardDrive size={23} />
              </div>
              <div className="scope-copy">
                <strong>
                  {mode === "all" ? "全盘扫描" : "自定义目录"}
                  <span className="tag">
                    {mode === "all"
                      ? preferencesSaved
                        ? "已保存"
                        : "默认范围"
                      : `${customRoots.length} 个目录${preferencesSaved ? " · 已保存" : ""}`}
                  </span>
                </strong>
                <p>
                  {mode === "all"
                    ? `自动扫描所有已挂载磁盘${drives.length ? ` · 已检测到 ${drives.length} 个磁盘` : ""}`
                    : "扫描你选择的项目目录"}
                </p>
              </div>
              <button
                className="text-button scope-change"
                disabled={busy}
                onClick={() => setSettings(true)}
              >
                调整范围
                <ChevronRight size={14} />
              </button>
              {scanning ? (
                <button
                  className="button secondary"
                  onClick={cancel}
                  disabled={cancelling}
                >
                  <Square size={14} />
                  {cancelling ? "正在停止…" : "停止扫描"}
                </button>
              ) : (
                <button
                  className="button primary"
                  onClick={start}
                  disabled={busy || !enabledKinds.size}
                >
                  <Play size={15} fill="currentColor" />
                  {summary ? "重新扫描" : "开始扫描"}
                </button>
              )}
            </div>
            <div className="scan-bottom">
              <div className="type-toggles">
                <span>扫描类型</span>
                {kinds.map((k) => (
                  <label
                    key={k}
                    className={`type-chip ${enabledKinds.has(k) ? "enabled" : ""}`}
                  >
                    <input
                      type="checkbox"
                      checked={enabledKinds.has(k)}
                      disabled={busy}
                      onChange={() =>
                        setEnabledKinds((old) => {
                          const next = new Set(old);
                          if (next.has(k)) next.delete(k);
                          else next.add(k);
                          return next;
                        })
                      }
                    />
                    <span className={`type-dot ${k}`} />
                    {kindInfo[k].dir}
                    {enabledKinds.has(k) && <Check size={12} />}
                  </label>
                ))}
              </div>
              <span className="safe-hint">
                <ShieldCheck size={13} />
                先扫描，确认后再清理
              </span>
            </div>
          </section>

          <div className="metrics">
            <div className="metric main-metric">
              <div className="metric-label">
                <span>目录逻辑体积</span>
                <Layers3 size={16} />
              </div>
              <div className="metric-value">
                {size(scanning ? progress.bytes : remainingBytes)}
                <small>
                  {scanning
                    ? "统计中"
                    : summary
                      ? failedItems.length
                        ? "待复核"
                        : summary.cancelled || progress.errors > 0
                          ? "部分统计"
                          : "已统计"
                      : "等待扫描"}
                </small>
              </div>
              <div className="metric-foot">
                {summary?.cancelled
                  ? "扫描已取消，以下为部分结果"
                  : failedItems.length
                    ? "清理未确认完成，请重新扫描核对大小"
                    : progress.errors > 0
                      ? "有目录无法读取，合计可能不完整"
                      : "实际释放空间取决于磁盘与回收站"}
              </div>
            </div>
            <div className="metric">
              <div className="metric-label">
                <span>剩余目录</span>
                <Folder size={16} />
              </div>
              <div className="metric-value">
                {remainingCount.toLocaleString()}
                <small>个目录</small>
              </div>
              <div className="language-summary">
                {kinds.map((k) => (
                  <span key={k}>
                    <i className={`type-dot ${k}`} />
                    {kindInfo[k].label}
                    <b>{totals[k].count}</b>
                  </span>
                ))}
              </div>
            </div>
            <div className="metric">
              <div className="metric-label">
                <span>扫描状态</span>
                <Cpu size={16} />
              </div>
              <div className={`status-value ${scanning ? "running" : ""}`}>
                {scanning ? (
                  <>
                    <LoaderCircle size={22} className="spin" />
                    {cancelling ? "正在停止" : "正在探索"}
                  </>
                ) : cleaning ? (
                  <>
                    <LoaderCircle size={22} className="spin" />
                    正在清理
                  </>
                ) : summary ? (
                  <>
                    <CheckCheck size={22} />
                    {summary.cancelled ? "已停止扫描" : "扫描已结束"}
                  </>
                ) : (
                  <>
                    <span className="status-dot" />
                    准备就绪
                  </>
                )}
              </div>
              <div className="metric-foot">
                {summary
                  ? `耗时 ${(summary.elapsed_ms / 1000).toFixed(2)} 秒 · `
                  : ""}
                {progress.directories.toLocaleString()} 个目录已检查
                {progress.errors > 0 && (
                  <button
                    className="inline-link"
                    onClick={() => setShowIssues(true)}
                  >
                    {" "}
                    · {progress.errors} 项读取错误
                  </button>
                )}
              </div>
            </div>
          </div>

          <section className="results-panel" aria-label="扫描结果">
            <div className="results-heading">
              <h2>
                扫描结果<span>{visible.length}</span>
              </h2>
              <div className="result-tabs">
                <div className="tabs">
                  {(["all", ...kinds] as const).map((k) => (
                    <button
                      className={filter === k ? "selected" : ""}
                      key={k}
                      onClick={() => setFilter(k)}
                    >
                      {k === "all" ? "全部目录" : kindInfo[k].label}
                      <span>
                        {k === "all" ? allItems.length : totals[k].count}
                      </span>
                    </button>
                  ))}
                </div>
              </div>
              <div className="results-tools">
                <label className="search">
                  <Search size={15} />
                  <input
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    placeholder="搜索项目或路径…"
                    aria-label="搜索项目或路径"
                  />
                  {query && (
                    <button
                      className="icon-button"
                      aria-label="清空搜索"
                      onClick={() => setQuery("")}
                    >
                      <X size={13} />
                    </button>
                  )}
                </label>
                <button
                  className="sort-button"
                  title="切换大小或项目名称排序"
                  onClick={() => setSort(sort === "size" ? "name" : "size")}
                >
                  <ArrowDownWideNarrow size={15} />
                  {sort === "size" ? "大小" : "名称"}
                </button>
              </div>
            </div>
            <div className="table-header result-grid">
              <label className="check-wrap">
                <input
                  type="checkbox"
                  checked={allChecked}
                  onChange={toggleAll}
                  disabled={busy || !eligible.length}
                  aria-label="选择当前筛选下全部完整结果"
                />
              </label>
              <span>项目 / 路径</span>
              <span>类型</span>
              <span className="right">逻辑大小</span>
              <span className="right">修改日期</span>
              <span />
            </div>
            <div className="result-scroll" ref={scrollRef}>
              {visible.length ? (
                <div
                  style={{
                    height: virtualizer.getTotalSize(),
                    position: "relative",
                  }}
                >
                  {virtualizer.getVirtualItems().map((row) => {
                    const item = visible[row.index];
                    const result = cleanResults.get(item.id);
                    return (
                      <div
                        key={item.id}
                        data-candidate-id={item.id}
                        tabIndex={0}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          showContextMenu(item.id, e.clientX, e.clientY);
                        }}
                        onKeyDown={(e) => {
                          if (
                            e.key === "ContextMenu" ||
                            (e.shiftKey && e.key === "F10")
                          ) {
                            e.preventDefault();
                            const r = e.currentTarget.getBoundingClientRect();
                            showContextMenu(item.id, r.left + 48, r.top + 20);
                          }
                        }}
                        className={`result-row result-grid ${selected.has(item.id) ? "checked" : ""} ${result?.success ? "cleaned" : ""}`}
                        style={{
                          position: "absolute",
                          top: 0,
                          left: 0,
                          width: "100%",
                          height: row.size,
                          transform: `translateY(${row.start}px)`,
                        }}
                      >
                        <label className="check-wrap">
                          <input
                            type="checkbox"
                            checked={selected.has(item.id)}
                            onChange={() => toggleSelected(item.id)}
                            disabled={busy || !selectable(item)}
                            aria-label={`选择 ${item.project} 的 ${kindInfo[item.kind].dir}`}
                          />
                        </label>
                        <button
                          className="project-cell"
                          onClick={() => setDetail(item)}
                        >
                          <span className={`folder-icon ${item.kind}`}>
                            <Folder size={20} />
                          </span>
                          <span>
                            <strong>
                              {item.project}
                              <span className="folder-suffix">
                                {" "}
                                / {kindInfo[item.kind].dir}
                              </span>
                            </strong>
                            <small title={displayPath(item.path)}>
                              {displayPath(item.path)}
                            </small>
                            {result && (
                              <em
                                className={
                                  result.success ? "success-text" : "error-text"
                                }
                              >
                                {result.success
                                  ? "已移入回收站"
                                  : "清理失败 · 查看详情"}
                              </em>
                            )}
                          </span>
                        </button>
                        <span>
                          <span className={`kind-badge ${item.kind}`}>
                            {kindInfo[item.kind].label}
                          </span>
                        </span>
                        <span className="row-size right">
                          {item.measured ? (
                            size(item.bytes)
                          ) : (
                            <span className="muted">计算中…</span>
                          )}
                          {item.measured && !item.complete && (
                            <small className="error-text">部分统计</small>
                          )}
                        </span>
                        <span className="row-date right">
                          {item.modified
                            ? new Date(item.modified * 1000).toLocaleDateString(
                                "zh-CN",
                              )
                            : "—"}
                        </span>
                        <button
                          className="icon-button"
                          onClick={() => setDetail(item)}
                          aria-label={`查看 ${item.project} 的详情`}
                        >
                          <ArrowUpRight size={16} />
                        </button>
                      </div>
                    );
                  })}
                </div>
              ) : (
                <div className="empty-state">
                  <div className="empty-art">
                    <div />
                    <FolderOpen size={38} strokeWidth={1.15} />
                    <span>
                      <Search size={17} />
                    </span>
                  </div>
                  <h3>
                    {query || filter !== "all"
                      ? "没有匹配的目录"
                      : scanning
                        ? "正在寻找开发目录"
                        : summary
                          ? "当前范围没有发现目标目录"
                          : "好项目，值得一个清爽的空间"}
                  </h3>
                  <p>
                    {query || filter !== "all"
                      ? "试试其他关键词，或切换到全部目录。"
                      : scanning
                        ? "结果会陆续出现，目录大小会在统计后补充。"
                        : summary
                          ? "可以调整范围或扫描类型后重试；有读取错误时请查看详情。"
                          : "从一次扫描开始，看看依赖与构建产物占了多少空间。"}
                  </p>
                  {!summary && !scanning && !query && filter === "all" && (
                    <div className="empty-tags">
                      <span>
                        <Code2 size={13} />
                        三种开发环境
                      </span>
                      <span>
                        <HardDrive size={13} />
                        所有已挂载磁盘
                      </span>
                      <span>
                        <ShieldCheck size={13} />
                        手动确认清理
                      </span>
                    </div>
                  )}
                </div>
              )}
            </div>
            <div className="scan-status-line">
              {scanning ? (
                <>
                  <LoaderCircle size={13} className="spin" />
                  <span title={currentPath}>{currentPath}</span>
                  <span>
                    {finishedRoots} / {rootList.length} 个磁盘已完成
                  </span>
                </>
              ) : (
                <>
                  <span className="status-dot" />
                  <span>
                    {summary
                      ? `${summary.cancelled ? "部分扫描" : "本次扫描"} · ${progress.files.toLocaleString()} 个目标文件已统计 · 跳过 ${progress.skipped_links} 个链接`
                      : "扫描不会修改你的任何文件"}
                  </span>
                  {issues.length > 0 && (
                    <button
                      className="inline-link"
                      onClick={() => setShowIssues(true)}
                    >
                      查看读取问题
                    </button>
                  )}
                </>
              )}
            </div>
          </section>

          <div className="page-footnote">
            <ShieldCheck size={14} />
            <span>
              依赖可以重新安装，构建产物可以重新生成。清理前，请确认没有需要保留的文件。
            </span>
            <span>
              BUILT WITH RUST<span className="footnote-dot">·</span>LOCAL FIRST
            </span>
          </div>
        </section>
        <footer className="selection-bar">
          <div className="selection-summary">
            <span className="selection-icon">
              <Check size={17} />
            </span>
            <span>
              已选择 <strong>{chosen.length}</strong> 个目录
            </span>
            <span className="footer-divider" />
            <span>
              逻辑体积{" "}
              <strong className="selected-size">{size(chosenBytes)}</strong>
            </span>
            {chosen.length > 0 && (
              <button
                className="text-button"
                disabled={busy}
                onClick={() => setSelected(new Set())}
              >
                取消选择
              </button>
            )}
          </div>
          <button
            className="button clean-button"
            disabled={busy || !chosen.length}
            onClick={() => setConfirm(true)}
          >
            {cleaning ? (
              <LoaderCircle size={16} className="spin" />
            ) : (
              <Trash2 size={16} />
            )}
            {cleaning ? "正在移入回收站…" : "清理所选目录"}
            {!cleaning && <ChevronRight size={15} />}
          </button>
        </footer>
      </main>

      {contextMenu && contextItem && (
        <div
          ref={menuRef}
          className="result-context-menu"
          role="menu"
          aria-label="目录操作"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onContextMenu={(e) => e.preventDefault()}
          onKeyDown={(e) => {
            if (["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) {
              e.preventDefault();
              const buttons = Array.from(
                e.currentTarget.querySelectorAll<HTMLButtonElement>(
                  "button:not(:disabled)",
                ),
              );
              const at = buttons.indexOf(
                document.activeElement as HTMLButtonElement,
              );
              const next =
                e.key === "Home"
                  ? 0
                  : e.key === "End"
                    ? buttons.length - 1
                    : (at + (e.key === "ArrowUp" ? -1 : 1) + buttons.length) %
                      buttons.length;
              buttons[next]?.focus();
            }
          }}
        >
          <div className="context-caption">
            {contextItem.project} / {kindInfo[contextItem.kind].dir}
          </div>
          <button
            autoFocus={!cleaning && !removed.has(contextItem.id)}
            role="menuitem"
            disabled={cleaning || removed.has(contextItem.id)}
            onClick={() => openDirectory(contextItem)}
          >
            <FolderOpen size={15} />
            打开目录
          </button>
          <button
            role="menuitem"
            autoFocus={cleaning || removed.has(contextItem.id)}
            onClick={() => {
              setDetail(contextItem);
              setContextMenu(null);
            }}
          >
            <ArrowUpRight size={15} />
            查看详情
          </button>
        </div>
      )}

      {cleanReport && (
        <Modal title="清理失败目录" onClose={() => setCleanReport(false)} wide>
          <div className="modal-body">
            <p className="modal-description">
              以下目录未确认完成回收。请检查具体原因后重新扫描或重试清理。
            </p>
            <div className="issue-list">
              {failedItems.map((item) => (
                <div key={item.id}>
                  <code>{displayPath(item.path)}</code>
                  <p className="error-text">
                    {cleanResults.get(item.id)?.message}
                  </p>
                  <button
                    className="button secondary"
                    onClick={() => {
                      setCleanReport(false);
                      void openDirectory(item);
                    }}
                  >
                    <FolderOpen size={14} />
                    打开目录
                  </button>
                </div>
              ))}
            </div>
          </div>
          <div className="modal-footer">
            <button
              className="button secondary"
              onClick={() => setCleanReport(false)}
            >
              关闭
            </button>
          </div>
        </Modal>
      )}

      {settings && (
        <Modal title="扫描设置" onClose={() => setSettings(false)} wide>
          <div className="modal-body">
            <p className="modal-description">
              选择扫描范围后点击“保存设置”，下次启动会自动恢复目录、扫描类型和线程数。
            </p>
            <div className="scope-options">
              <button
                disabled={busy}
                className={mode === "all" ? "chosen" : ""}
                onClick={() => setMode("all")}
              >
                <HardDrive size={20} />
                <strong>所有磁盘</strong>
                <small>本地、外置与已映射网络盘</small>
                {mode === "all" && <Check size={16} />}
              </button>
              <button
                disabled={busy}
                className={mode === "custom" ? "chosen" : ""}
                onClick={() => setMode("custom")}
              >
                <FolderOpen size={20} />
                <strong>自定义目录</strong>
                <small>只扫描你选择的位置</small>
                {mode === "custom" && <Check size={16} />}
              </button>
            </div>
            <div className="settings-section">
              <h3>{mode === "all" ? "检测到的磁盘" : "自定义目录"}</h3>
              {!rootList.length && (
                <p className="muted">
                  {native
                    ? "暂无目录，选择目录或重新打开应用以刷新磁盘列表。"
                    : "桌面应用会在此显示本机磁盘。"}
                </p>
              )}
              {rootList.map((path) => (
                <div className="drive-row" key={path}>
                  <HardDrive size={16} />
                  <span title={path}>{displayPath(path)}</span>
                  <small title={rootStatuses[displayPath(path)]?.reason}>
                    {rootStatuses[displayPath(path)]?.phase === "done"
                      ? rootStatuses[displayPath(path)]?.errors
                        ? "已结束 · 有读取错误"
                        : "已结束"
                      : rootStatuses[displayPath(path)]?.phase === "scanning"
                        ? "扫描中"
                        : rootStatuses[displayPath(path)]?.phase === "skipped"
                          ? "已跳过"
                          : rootStatuses[displayPath(path)]?.phase ===
                              "cancelled"
                            ? "已停止"
                            : "待扫描"}
                  </small>
                  {mode === "custom" && (
                    <button
                      className="icon-button"
                      disabled={busy}
                      aria-label={`移除 ${path}`}
                      onClick={() =>
                        setCustomRoots((old) => old.filter((p) => p !== path))
                      }
                    >
                      <X size={15} />
                    </button>
                  )}
                </div>
              ))}
              {mode === "custom" && (
                <button
                  className="button secondary add-folder"
                  onClick={addFolder}
                  disabled={busy}
                >
                  <Plus size={15} />
                  添加目录
                </button>
              )}
            </div>
            <div className="settings-section">
              <h3>扫描并发</h3>
              <label className="worker-control">
                <span>工作线程</span>
                <select
                  value={workers}
                  disabled={busy}
                  onChange={(e) => setWorkers(Number(e.target.value))}
                >
                  {![1, 2, 4, 8, 16].includes(workers) && (
                    <option value={workers}>{workers}</option>
                  )}
                  {[1, 2, 4, 8, 16].map((n) => (
                    <option key={n} value={n}>
                      {n}
                      {n === 8 ? "（默认）" : ""}
                    </option>
                  ))}
                </select>
              </label>
              <p className="small muted">
                更多线程不一定更快。机械硬盘与网络盘建议使用 1–2 个线程，SSD
                可尝试 4–8 个。
              </p>
            </div>
            <p className="small muted">
              系统目录与链接会跳过。磁盘未就绪或权限不足时记录错误并继续其他磁盘。
            </p>
            {preferencesError && (
              <p className="error-text small" role="alert">
                {preferencesError}
              </p>
            )}
          </div>
          <div className="modal-footer">
            <span className="settings-save-status" role="status">
              {!preferencesReady
                ? "正在读取设置…"
                : preferencesSaved
                  ? "已保存，下次启动自动恢复"
                  : "本次更改尚未保存"}
            </span>
            <button
              className="button secondary"
              disabled={savingPreferences}
              onClick={() => setSettings(false)}
            >
              关闭
            </button>
            <button
              className="button primary"
              disabled={
                busy ||
                !enabledKinds.size ||
                (mode === "custom" && !customRoots.length)
              }
              onClick={rememberSettings}
            >
              {savingPreferences ? "保存中…" : "保存设置"}
            </button>
          </div>
        </Modal>
      )}

      {detail && (
        <Modal title="目录详情" onClose={() => setDetail(null)}>
          <div className="modal-body">
            <span className={`kind-badge ${detail.kind}`}>
              {kindInfo[detail.kind].label}
            </span>
            <h3 className="detail-title">
              {detail.project} / {kindInfo[detail.kind].dir}
            </h3>
            <code className="detail-path">{displayPath(detail.path)}</code>
            <dl className="detail-list">
              <dt>逻辑大小</dt>
              <dd>{detail.measured ? size(detail.bytes) : "计算中"}</dd>
              <dt>统计状态</dt>
              <dd>
                {!detail.measured
                  ? "正在统计"
                  : detail.complete
                    ? "已完成"
                    : "不完整，不能清理"}
              </dd>
              <dt>文件数量</dt>
              <dd>{detail.files.toLocaleString()}</dd>
              <dt>跳过的链接</dt>
              <dd>{detail.skipped_links}</dd>
              <dt>读取错误</dt>
              <dd>{detail.errors}</dd>
              <dt>识别依据</dt>
              <dd>{detail.evidence}</dd>
              <dt>扫描根目录</dt>
              <dd className="break-path">{displayPath(detail.root)}</dd>
            </dl>
            {cleanResults.get(detail.id) && (
              <p
                className={
                  cleanResults.get(detail.id)?.success
                    ? "success-text"
                    : "error-text"
                }
              >
                {cleanResults.get(detail.id)?.message}
              </p>
            )}
            <div className="detail-note">
              <strong>统计说明</strong>
              <p className="small muted">
                大小为普通文件的逻辑字节总和，跳过链接；硬链接、文件压缩和扫描时文件变化会影响实际磁盘占用。修改日期是目录修改时间，并非最后使用时间。
              </p>
            </div>
          </div>
        </Modal>
      )}

      {confirm && (
        <Modal title="将所选目录移入回收站？" onClose={() => setConfirm(false)}>
          <div className="modal-body">
            <div className="confirm-total">
              <Trash2 size={25} />
              <div>
                <strong>{size(chosenBytes)}</strong>
                <span>{chosen.length} 个开发目录</span>
              </div>
            </div>
            <p>
              清理后，相应项目可能需要重新安装依赖、重建虚拟环境或重新编译。请确认其中没有手动存放的重要文件。
            </p>
            {chosen.some((i) => i.kind === "python") && (
              <p className="warning">
                包含 Python 虚拟环境，其已安装的包和环境内文件将一并移入回收站。
              </p>
            )}
            <div className="confirm-list">
              {chosen.map((i) => (
                <div key={i.id}>
                  <Folder size={14} />
                  <span title={displayPath(i.path)}>{displayPath(i.path)}</span>
                  <strong>{size(i.bytes)}</strong>
                </div>
              ))}
            </div>
            <p className="small muted">
              清空回收站后才会真正释放空间。不支持回收站的磁盘会报告失败，不会永久删除。
            </p>
          </div>
          <div className="modal-footer">
            <button
              className="button secondary"
              onClick={() => setConfirm(false)}
            >
              返回检查
            </button>
            <button className="button primary" onClick={cleanSelected}>
              <Trash2 size={15} />
              确认移入回收站
            </button>
          </div>
        </Modal>
      )}

      {showIssues && (
        <Modal title="扫描读取问题" onClose={() => setShowIssues(false)} wide>
          <div className="modal-body">
            <p className="modal-description">
              共 {progress.errors} 项读取错误，展示前 100
              项。受影响的目录可能无法完整统计。
            </p>
            <div className="issue-list">
              {issues.map((issue, i) => (
                <div key={i}>
                  <code>{displayPath(issue.path) || "扫描任务"}</code>
                  <p>{issue.message}</p>
                </div>
              ))}
            </div>
          </div>
        </Modal>
      )}

      {help && (
        <Modal title="把空间留给下一个项目" onClose={() => setHelp(false)}>
          <div className="modal-body help-body">
            <Sparkles size={28} className="accent" />
            <p>
              缓境寻找项目里的 <code>node_modules</code>、<code>target</code> 和{" "}
              <code>.venv</code>
              ，并通过项目标识确认目录类型。同名目录不一定会被识别。
            </p>
            <h3>默认扫描所有磁盘</h3>
            <p>
              包括已挂载的外置盘与映射网络盘。扫描中可以停止；无法读取的目录会被记录，不会影响其他磁盘。
            </p>
            <h3>体积不是实际释放空间</h3>
            <p>
              统计显示文件逻辑大小，并跳过链接。权限不足、文件变化、硬链接与压缩都会影响完整性或真实占用。移入回收站后，需清空回收站才会释放空间。
            </p>
            <h3>清理始终由你决定</h3>
            <p>
              默认不勾选结果。正在统计或统计不完整的目录不能清理。清理失败时保留目录并展示错误。
            </p>
            <div className="help-footer">
              <Clock3 size={15} />
              文件夹修改时间不代表项目最后使用时间。
            </div>
          </div>
        </Modal>
      )}
    </div>
  );
}
