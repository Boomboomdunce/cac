import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

interface AppStatus {
  active: boolean;
  paused: boolean;
  profile: string | null;
  version: string;
}

export default function StatusBar() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<AppStatus | null>(null);

  useEffect(() => {
    invoke<AppStatus>("get_status").then(setStatus).catch(() => {});
  }, []);

  const dotColor = status?.active
    ? "bg-green-500"
    : status?.paused
      ? "bg-yellow-500"
      : "bg-gray-400";

  const label = status?.active
    ? t("status.running")
    : t("status.stopped");

  return (
    <div className="h-8 px-4 flex items-center gap-4 text-xs text-gray-500 dark:text-gray-400 border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800">
      <span className="flex items-center gap-1.5">
        <span className={`w-2 h-2 rounded-full ${dotColor}`} />
        {label}
      </span>
      <span>{status?.profile ?? t("status.noProfile")}</span>
      <span className="ml-auto">v{status?.version ?? "?"}</span>
    </div>
  );
}
