import { useTranslation } from "react-i18next";

export default function TrafficCapture() {
  const { t } = useTranslation();

  return (
    <div>
      <h1 className="text-xl font-semibold mb-6">{t("traffic.title")}</h1>

      {/* Control Bar */}
      <div className="flex items-center gap-3 mb-4">
        <button className="px-4 py-1.5 bg-green-600 text-white text-sm rounded-lg hover:bg-green-700 transition-colors">
          {t("traffic.startCapture")}
        </button>
        <button className="px-4 py-1.5 bg-gray-200 dark:bg-gray-700 text-sm rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors" disabled>
          {t("traffic.pauseCapture")}
        </button>
        <button className="px-4 py-1.5 bg-gray-200 dark:bg-gray-700 text-sm rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors" disabled>
          {t("traffic.clearCapture")}
        </button>
        <div className="ml-auto text-xs text-gray-400">
          {t("traffic.memory")}: 0 MB / 1024 MB
        </div>
      </div>

      {/* Request Table Placeholder */}
      <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 flex-1 min-h-[400px] flex items-center justify-center text-gray-400 text-sm">
        {t("traffic.startCapture")}
      </div>

      {/* Stats Bar */}
      <div className="flex items-center gap-6 mt-4 text-xs text-gray-500 dark:text-gray-400">
        <span>{t("traffic.totalRequests")}: 0</span>
        <span>{t("traffic.allowed")}: 0</span>
        <span>{t("traffic.blocked")}: 0</span>
        <span>{t("traffic.avgLatency")}: --</span>
      </div>
    </div>
  );
}
