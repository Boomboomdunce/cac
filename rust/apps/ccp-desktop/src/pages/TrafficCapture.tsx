import { useState, useRef, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import RequestDetail from "../components/RequestDetail";

export interface CapturedRequest {
  id: number;
  timestamp: string;
  tool: string;
  method: string;
  url: string;
  status: number | null;
  size: number;
  duration: number | null;
  category: "normal" | "blocked" | "telemetry";
  blocked_reason: string | null;
  request_headers: [string, string][];
  request_body: string | null;
  response_headers: [string, string][];
  response_body: string | null;
}

type SortKey = "id" | "timestamp" | "method" | "url" | "status" | "size" | "duration";
type SortDir = "asc" | "desc";

export default function TrafficCapture() {
  const { t } = useTranslation();
  const [capturing, setCapturing] = useState(false);
  const [proxyPort, setProxyPort] = useState<number | null>(null);
  const [requests, setRequests] = useState<CapturedRequest[]>([]);
  const [selected, setSelected] = useState<CapturedRequest | null>(null);
  const [filter, setFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState<"all" | "normal" | "blocked">("all");
  const [sortKey, setSortKey] = useState<SortKey>("id");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; req: CapturedRequest } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const tableRef = useRef<HTMLDivElement>(null);
  const autoScroll = useRef(true);

  const memoryMb = (requests.length * 1024) / (1024 * 1024);

  // Check initial capture status
  useEffect(() => {
    invoke<{ running: boolean; port: number | null }>("get_capture_status").then((s) => {
      setCapturing(s.running);
      setProxyPort(s.port);
      if (s.running) {
        // Load existing captured data
        invoke<CapturedRequest[]>("get_capture_snapshot").then(setRequests).catch(() => {});
      }
    }).catch(() => {});
  }, []);

  // Listen for real-time capture events from Tauri backend
  useEffect(() => {
    const unlisten = listen<CapturedRequest>("capture-request", (event) => {
      setRequests((prev) => [...prev, event.payload]);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const startCapture = async () => {
    setError(null);
    try {
      const port = await invoke<number>("start_capture");
      setCapturing(true);
      setProxyPort(port);
    } catch (e) {
      setError(String(e));
    }
  };

  const stopCapture = async () => {
    try {
      await invoke("stop_capture");
      setCapturing(false);
      setProxyPort(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const clearCapture = async () => {
    try {
      await invoke("clear_capture_buffer");
      setRequests([]);
      setSelected(null);
    } catch {
      // ignore
    }
  };

  // Close context menu on any click
  useEffect(() => {
    const close = () => setCtxMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, []);

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir(sortDir === "asc" ? "desc" : "asc");
    } else {
      setSortKey(key);
      setSortDir("asc");
    }
  };

  const filteredRequests = requests
    .filter((r) => {
      if (typeFilter === "normal" && r.category !== "normal") return false;
      if (typeFilter === "blocked" && r.category !== "blocked") return false;
      if (filter && !r.url.toLowerCase().includes(filter.toLowerCase())) return false;
      return true;
    })
    .sort((a, b) => {
      const dir = sortDir === "asc" ? 1 : -1;
      const av = a[sortKey] ?? 0;
      const bv = b[sortKey] ?? 0;
      if (typeof av === "string" && typeof bv === "string") return av.localeCompare(bv) * dir;
      return ((av as number) - (bv as number)) * dir;
    });

  useEffect(() => {
    if (autoScroll.current && tableRef.current) {
      tableRef.current.scrollTop = tableRef.current.scrollHeight;
    }
  }, [filteredRequests.length]);

  const handleScroll = useCallback(() => {
    if (!tableRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = tableRef.current;
    autoScroll.current = scrollHeight - scrollTop - clientHeight < 50;
  }, []);

  const handleContextMenu = (e: React.MouseEvent, req: CapturedRequest) => {
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY, req });
  };

  const copyText = (text: string) => {
    navigator.clipboard.writeText(text);
    setCtxMenu(null);
  };

  const copyAsCurl = (req: CapturedRequest) => {
    let curl = `curl '${req.url}'`;
    for (const [k, v] of req.request_headers) {
      curl += ` \\\n  -H '${k}: ${v}'`;
    }
    if (req.request_body) {
      curl += ` \\\n  --data-raw '${req.request_body}'`;
    }
    copyText(curl);
  };

  const stats = {
    total: requests.length,
    allowed: requests.filter((r) => r.category === "normal").length,
    blocked: requests.filter((r) => r.category === "blocked").length,
    avgLatency:
      requests.filter((r) => r.duration != null).length > 0
        ? Math.round(
            requests.reduce((a, b) => a + (b.duration ?? 0), 0) /
              requests.filter((r) => r.duration != null).length,
          )
        : 0,
  };

  const SortHeader = ({ k, children, className }: { k: SortKey; children: React.ReactNode; className?: string }) => (
    <th
      className={`px-3 py-2 text-left text-xs font-medium text-gray-500 cursor-pointer hover:text-gray-700 select-none ${className ?? ""}`}
      onClick={() => handleSort(k)}
    >
      {children}
      {sortKey === k && <span className="ml-0.5">{sortDir === "asc" ? "\u25B2" : "\u25BC"}</span>}
    </th>
  );

  return (
    <div className="flex flex-col h-full">
      <h1 className="text-xl font-semibold mb-4">{t("traffic.title")}</h1>

      {/* Error */}
      {error && (
        <div className="mb-3 p-3 bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400 rounded-lg text-sm">
          {error}
        </div>
      )}

      {/* Control Bar */}
      <div className="flex items-center gap-3 mb-3 flex-wrap">
        {!capturing ? (
          <button onClick={startCapture} className="px-4 py-1.5 bg-green-600 text-white text-sm rounded-lg hover:bg-green-700">
            {t("traffic.startCapture")}
          </button>
        ) : (
          <button onClick={stopCapture} className="px-4 py-1.5 bg-red-500 text-white text-sm rounded-lg hover:bg-red-600">
            {t("traffic.stopCapture")}
          </button>
        )}
        <button onClick={clearCapture} disabled={requests.length === 0} className="px-4 py-1.5 bg-gray-200 dark:bg-gray-700 text-sm rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 disabled:opacity-50">
          {t("traffic.clearCapture")}
        </button>
        <select value={typeFilter} onChange={(e) => setTypeFilter(e.target.value as typeof typeFilter)} className="px-2 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700">
          <option value="all">{t("traffic.filterAll")}</option>
          <option value="normal">{t("traffic.filterAllowed")}</option>
          <option value="blocked">{t("traffic.filterBlocked")}</option>
        </select>
        <input type="text" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder={t("traffic.searchPlaceholder")} className="px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 flex-1 min-w-[200px] focus:ring-2 focus:ring-green-500 outline-none" />
        <div className="text-xs text-gray-400 whitespace-nowrap">
          {capturing && proxyPort && (
            <span className="mr-3 text-green-600">
              {t("traffic.listeningOn")} 127.0.0.1:{proxyPort}
            </span>
          )}
          {t("traffic.memory")}: {memoryMb.toFixed(1)} MB
        </div>
      </div>

      {/* Proxy usage hint */}
      {capturing && proxyPort && (
        <div className="mb-3 px-4 py-2 bg-blue-50 dark:bg-blue-900/20 rounded-lg text-xs text-blue-700 dark:text-blue-400">
          {t("traffic.proxyHint", { port: proxyPort })}
        </div>
      )}

      {/* Request Table */}
      <div ref={tableRef} onScroll={handleScroll} className="flex-1 min-h-0 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 overflow-auto">
        <table className="w-full text-sm">
          <thead className="sticky top-0 bg-gray-50 dark:bg-gray-750 border-b border-gray-200 dark:border-gray-700">
            <tr>
              <SortHeader k="id" className="w-12">#</SortHeader>
              <SortHeader k="timestamp" className="w-24">{t("traffic.colTime")}</SortHeader>
              <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 w-16">{t("traffic.colTool")}</th>
              <SortHeader k="method" className="w-20">{t("traffic.colMethod")}</SortHeader>
              <SortHeader k="url">{t("traffic.colUrl")}</SortHeader>
              <SortHeader k="status" className="w-16">{t("traffic.colStatus")}</SortHeader>
              <SortHeader k="size" className="w-16 text-right">{t("traffic.colSize")}</SortHeader>
              <SortHeader k="duration" className="w-20 text-right">{t("traffic.colDuration")}</SortHeader>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-50 dark:divide-gray-700/30">
            {filteredRequests.length === 0 && (
              <tr>
                <td colSpan={8} className="px-3 py-12 text-center text-gray-400">
                  {capturing ? t("traffic.waitingForRequests") : t("traffic.startCaptureHint")}
                </td>
              </tr>
            )}
            {filteredRequests.map((req) => (
              <tr
                key={req.id}
                onClick={() => setSelected(req)}
                onContextMenu={(e) => handleContextMenu(e, req)}
                className={`cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-750 ${selected?.id === req.id ? "bg-green-50 dark:bg-green-900/20" : ""} ${req.category === "blocked" ? "opacity-60" : ""}`}
              >
                <td className="px-3 py-1.5 text-xs text-gray-400">{req.id}</td>
                <td className="px-3 py-1.5 text-xs font-mono">{req.timestamp}</td>
                <td className="px-3 py-1.5 text-xs">{req.tool}</td>
                <td className="px-3 py-1.5 text-xs font-medium">{req.method}</td>
                <td className="px-3 py-1.5 text-xs truncate max-w-[300px]">
                  {req.category === "blocked" ? (
                    <><span className="line-through text-gray-400">{req.url}</span><span className="ml-1 text-red-400">blocked</span></>
                  ) : req.url}
                </td>
                <td className="px-3 py-1.5 text-xs">
                  {req.category === "blocked" ? <span className="text-red-400">BLK</span> : <StatusBadge code={req.status} />}
                </td>
                <td className="px-3 py-1.5 text-xs text-right text-gray-500">{req.category === "blocked" ? "--" : formatSize(req.size)}</td>
                <td className="px-3 py-1.5 text-xs text-right text-gray-500">{req.duration != null ? `${req.duration}ms` : "--"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Context Menu */}
      {ctxMenu && (
        <div
          className="fixed bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-xl z-50 py-1 min-w-[160px]"
          style={{ left: ctxMenu.x, top: ctxMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <CtxItem onClick={() => copyText(ctxMenu.req.url)}>{t("traffic.copyUrl")}</CtxItem>
          <CtxItem onClick={() => copyAsCurl(ctxMenu.req)}>{t("traffic.copyCurl")}</CtxItem>
          <CtxItem onClick={() => copyText(JSON.stringify(ctxMenu.req, null, 2))}>{t("traffic.copyJson")}</CtxItem>
        </div>
      )}

      {/* Detail Panel */}
      {selected && <RequestDetail request={selected} onClose={() => setSelected(null)} />}

      {/* Stats Bar */}
      <div className="flex items-center gap-6 mt-3 text-xs text-gray-500 dark:text-gray-400">
        <span>{t("traffic.totalRequests")}: {stats.total}</span>
        <span>{t("traffic.allowed")}: {stats.allowed}</span>
        <span>{t("traffic.blocked")}: {stats.blocked}</span>
        <span>{t("traffic.avgLatency")}: {stats.avgLatency > 0 ? `${stats.avgLatency}ms` : "--"}</span>
      </div>
    </div>
  );
}

function CtxItem({ children, onClick }: { children: React.ReactNode; onClick: () => void }) {
  return (
    <button onClick={onClick} className="block w-full text-left px-3 py-1.5 text-xs hover:bg-gray-50 dark:hover:bg-gray-700">
      {children}
    </button>
  );
}

function StatusBadge({ code }: { code: number | null }) {
  if (code == null) return <span className="text-gray-400">--</span>;
  const color = code < 300 ? "text-green-600" : code < 400 ? "text-blue-500" : code < 500 ? "text-yellow-600" : "text-red-600";
  return <span className={`font-mono ${color}`}>{code}</span>;
}

function formatSize(bytes: number): string {
  if (bytes === 0) return "--";
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}K`;
  return `${(bytes / (1024 * 1024)).toFixed(1)}M`;
}
