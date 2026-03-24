import { useState, useRef, useEffect, useCallback } from "react";
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

export default function TrafficCapture() {
  const { t } = useTranslation();
  const [capturing, setCapturing] = useState(false);
  const [paused, setPaused] = useState(false);
  const [requests, setRequests] = useState<CapturedRequest[]>([]);
  const [selected, setSelected] = useState<CapturedRequest | null>(null);
  const [filter, setFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState<"all" | "normal" | "blocked">("all");
  const tableRef = useRef<HTMLDivElement>(null);
  const autoScroll = useRef(true);
  const nextId = useRef(1);

  // Memory estimation (rough: ~1KB per request without body)
  const memoryMb = (requests.length * 1024) / (1024 * 1024);

  const startCapture = () => {
    setCapturing(true);
    setPaused(false);
  };

  const pauseCapture = () => {
    setPaused(true);
  };

  const resumeCapture = () => {
    setPaused(false);
  };

  const clearCapture = () => {
    setRequests([]);
    setSelected(null);
    nextId.current = 1;
  };

  // Demo: simulate incoming requests when capturing (will be replaced by Tauri events)
  useEffect(() => {
    if (!capturing || paused) return;
    // In production, this will listen to Tauri events from the capture engine
    // For now, this is a placeholder that does nothing
    // The UI is fully functional — just waiting for real data
    return () => {};
  }, [capturing, paused]);

  const filteredRequests = requests.filter((r) => {
    if (typeFilter === "normal" && r.category !== "normal") return false;
    if (typeFilter === "blocked" && r.category !== "blocked") return false;
    if (filter && !r.url.toLowerCase().includes(filter.toLowerCase())) return false;
    return true;
  });

  // Auto-scroll to bottom
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

  const stats = {
    total: requests.length,
    allowed: requests.filter((r) => r.category === "normal").length,
    blocked: requests.filter((r) => r.category === "blocked").length,
    avgLatency:
      requests.filter((r) => r.duration != null).length > 0
        ? Math.round(
            requests.filter((r) => r.duration != null).reduce((a, b) => a + (b.duration ?? 0), 0) /
              requests.filter((r) => r.duration != null).length,
          )
        : 0,
  };

  return (
    <div className="flex flex-col h-full">
      <h1 className="text-xl font-semibold mb-4">{t("traffic.title")}</h1>

      {/* Control Bar */}
      <div className="flex items-center gap-3 mb-3 flex-wrap">
        {!capturing ? (
          <button
            onClick={startCapture}
            className="px-4 py-1.5 bg-green-600 text-white text-sm rounded-lg hover:bg-green-700 transition-colors"
          >
            {t("traffic.startCapture")}
          </button>
        ) : (
          <>
            {paused ? (
              <button
                onClick={resumeCapture}
                className="px-4 py-1.5 bg-green-600 text-white text-sm rounded-lg hover:bg-green-700"
              >
                {t("traffic.resume")}
              </button>
            ) : (
              <button
                onClick={pauseCapture}
                className="px-4 py-1.5 bg-yellow-500 text-white text-sm rounded-lg hover:bg-yellow-600"
              >
                {t("traffic.pauseCapture")}
              </button>
            )}
          </>
        )}
        <button
          onClick={clearCapture}
          disabled={requests.length === 0}
          className="px-4 py-1.5 bg-gray-200 dark:bg-gray-700 text-sm rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 disabled:opacity-50"
        >
          {t("traffic.clearCapture")}
        </button>

        <select
          value={typeFilter}
          onChange={(e) => setTypeFilter(e.target.value as typeof typeFilter)}
          className="px-2 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700"
        >
          <option value="all">{t("traffic.filterAll")}</option>
          <option value="normal">{t("traffic.filterAllowed")}</option>
          <option value="blocked">{t("traffic.filterBlocked")}</option>
        </select>

        <input
          type="text"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder={t("traffic.searchPlaceholder")}
          className="px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 flex-1 min-w-[200px] focus:ring-2 focus:ring-green-500 outline-none"
        />

        <div className="text-xs text-gray-400 whitespace-nowrap">
          {t("traffic.memory")}: {memoryMb.toFixed(1)} MB / 1024 MB
        </div>
      </div>

      {/* Request Table */}
      <div
        ref={tableRef}
        onScroll={handleScroll}
        className="flex-1 min-h-0 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 overflow-auto"
      >
        <table className="w-full text-sm">
          <thead className="sticky top-0 bg-gray-50 dark:bg-gray-750 border-b border-gray-200 dark:border-gray-700">
            <tr>
              <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 w-12">#</th>
              <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 w-20">
                {t("traffic.colTime")}
              </th>
              <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 w-16">
                {t("traffic.colTool")}
              </th>
              <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 w-16">
                {t("traffic.colMethod")}
              </th>
              <th className="px-3 py-2 text-left text-xs font-medium text-gray-500">
                {t("traffic.colUrl")}
              </th>
              <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 w-16">
                {t("traffic.colStatus")}
              </th>
              <th className="px-3 py-2 text-right text-xs font-medium text-gray-500 w-16">
                {t("traffic.colSize")}
              </th>
              <th className="px-3 py-2 text-right text-xs font-medium text-gray-500 w-16">
                {t("traffic.colDuration")}
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-50 dark:divide-gray-700/30">
            {filteredRequests.length === 0 && (
              <tr>
                <td colSpan={8} className="px-3 py-12 text-center text-gray-400">
                  {capturing
                    ? t("traffic.waitingForRequests")
                    : t("traffic.startCaptureHint")}
                </td>
              </tr>
            )}
            {filteredRequests.map((req) => (
              <tr
                key={req.id}
                onClick={() => setSelected(req)}
                className={`cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-750 ${
                  selected?.id === req.id ? "bg-green-50 dark:bg-green-900/20" : ""
                } ${req.category === "blocked" ? "opacity-60" : ""}`}
              >
                <td className="px-3 py-1.5 text-xs text-gray-400">{req.id}</td>
                <td className="px-3 py-1.5 text-xs font-mono">{req.timestamp}</td>
                <td className="px-3 py-1.5 text-xs">{req.tool}</td>
                <td className="px-3 py-1.5 text-xs font-medium">{req.method}</td>
                <td className="px-3 py-1.5 text-xs truncate max-w-[300px]">
                  {req.category === "blocked" ? (
                    <span className="line-through text-gray-400">
                      {req.url}
                    </span>
                  ) : (
                    req.url
                  )}
                  {req.category === "blocked" && (
                    <span className="ml-1 text-red-400" title={req.blocked_reason ?? ""}>
                      blocked
                    </span>
                  )}
                </td>
                <td className="px-3 py-1.5 text-xs">
                  {req.category === "blocked" ? (
                    <span className="text-red-400">BLK</span>
                  ) : (
                    <StatusBadge code={req.status} />
                  )}
                </td>
                <td className="px-3 py-1.5 text-xs text-right text-gray-500">
                  {req.category === "blocked" ? "--" : formatSize(req.size)}
                </td>
                <td className="px-3 py-1.5 text-xs text-right text-gray-500">
                  {req.duration != null ? `${req.duration}ms` : "--"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Detail Panel */}
      {selected && (
        <RequestDetail request={selected} onClose={() => setSelected(null)} />
      )}

      {/* Stats Bar */}
      <div className="flex items-center gap-6 mt-3 text-xs text-gray-500 dark:text-gray-400">
        <span>
          {t("traffic.totalRequests")}: {stats.total}
        </span>
        <span>
          {t("traffic.allowed")}: {stats.allowed}
        </span>
        <span>
          {t("traffic.blocked")}: {stats.blocked}
        </span>
        <span>
          {t("traffic.avgLatency")}: {stats.avgLatency > 0 ? `${stats.avgLatency}ms` : "--"}
        </span>
      </div>
    </div>
  );
}

function StatusBadge({ code }: { code: number | null }) {
  if (code == null) return <span className="text-gray-400">--</span>;
  const color =
    code < 300 ? "text-green-600" : code < 400 ? "text-blue-500" : code < 500 ? "text-yellow-600" : "text-red-600";
  return <span className={`font-mono ${color}`}>{code}</span>;
}

function formatSize(bytes: number): string {
  if (bytes === 0) return "--";
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}K`;
  return `${(bytes / (1024 * 1024)).toFixed(1)}M`;
}
