import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

interface DiagnosticCheck {
  name: string;
  status: string;
  message: string | null;
}

interface DiagnosticReport {
  ok: boolean;
  checks: DiagnosticCheck[];
}

interface AppStatus {
  profile: string | null;
}

export default function Diagnostics() {
  const { t } = useTranslation();
  const [report, setReport] = useState<DiagnosticReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [profile, setProfile] = useState<string | null>(null);

  useEffect(() => {
    invoke<AppStatus>("get_status").then((s) => setProfile(s.profile)).catch(() => {});
  }, []);

  const runDiag = async () => {
    if (!profile) return;
    setLoading(true);
    setError(null);
    try {
      const r = await invoke<DiagnosticReport>("run_diagnostics", { profileName: profile });
      setReport(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleExport = () => {
    if (!report) return;
    const blob = new Blob([JSON.stringify(report, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `ccp-diagnostics-${profile}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  if (!profile) {
    return (
      <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-8 text-center text-gray-400 text-sm">
        {t("diagnostics.noProfile")}
      </div>
    );
  }

  return (
    <div>
      <div className="flex items-center gap-3 mb-4">
        <button
          onClick={runDiag}
          disabled={loading}
          className="px-4 py-1.5 text-sm bg-green-600 text-white rounded-lg hover:bg-green-700 disabled:opacity-50"
        >
          {loading ? t("diagnostics.running") : t("diagnostics.run")}
        </button>
        {report && (
          <button
            onClick={handleExport}
            className="px-3 py-1.5 text-xs bg-gray-100 dark:bg-gray-700 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600"
          >
            {t("diagnostics.export")}
          </button>
        )}
        <span className="text-sm text-gray-500">
          {t("diagnostics.forProfile")}: <strong>{profile}</strong>
        </span>
      </div>

      {error && (
        <div className="mb-4 p-3 bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400 rounded-lg text-sm">
          {error}
        </div>
      )}

      {report && (
        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700">
          <div className="px-4 py-3 border-b border-gray-100 dark:border-gray-700 flex items-center gap-2">
            <span className={report.ok ? "text-green-500" : "text-red-500"}>
              {report.ok ? "\u2705" : "\u274C"}
            </span>
            <span className="text-sm font-medium">
              {report.ok ? t("diagnostics.allPassed") : t("diagnostics.issuesFound")}
            </span>
            <span className="ml-auto text-xs text-gray-400">
              {report.checks.filter((c) => c.status === "OK").length}/{report.checks.length}{" "}
              {t("diagnostics.passed")}
            </span>
          </div>
          <div className="divide-y divide-gray-50 dark:divide-gray-700/50">
            {report.checks.map((check, i) => (
              <div key={i} className="px-4 py-2.5 flex items-start gap-3">
                <span className="mt-0.5">
                  {check.status === "OK" && <StatusIcon color="green" />}
                  {check.status === "WARNING" && <StatusIcon color="yellow" />}
                  {check.status === "ERROR" && <StatusIcon color="red" />}
                </span>
                <div className="flex-1 min-w-0">
                  <div className="text-sm">{check.name}</div>
                  {check.message && (
                    <div className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 break-all">
                      {check.message}
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function StatusIcon({ color }: { color: "green" | "yellow" | "red" }) {
  const cls = {
    green: "bg-green-500",
    yellow: "bg-yellow-500",
    red: "bg-red-500",
  }[color];
  return <span className={`inline-block w-2.5 h-2.5 rounded-full ${cls}`} />;
}
