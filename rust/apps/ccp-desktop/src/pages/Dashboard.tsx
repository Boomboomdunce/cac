import { useTranslation } from "react-i18next";

export default function Dashboard() {
  const { t } = useTranslation();

  return (
    <div>
      <h1 className="text-xl font-semibold mb-6">{t("dashboard.title")}</h1>

      {/* Status Cards */}
      <div className="grid grid-cols-4 gap-4 mb-8">
        <Card label={t("status.protection")} value={t("status.stopped")} color="gray" />
        <Card label={t("status.profile")} value={t("status.noProfile")} color="gray" />
        <Card label={t("status.egressIp")} value="--" color="gray" />
        <Card label={t("status.uptime")} value="--" color="gray" />
      </div>

      {/* Proxied Tools */}
      <section className="mb-8">
        <h2 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-3">
          {t("dashboard.proxiedTools")}
        </h2>
        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-8 text-center text-gray-400 text-sm">
          {t("dashboard.noToolsRunning")}
        </div>
      </section>

      {/* Protection Layers */}
      <section>
        <h2 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-3">
          {t("dashboard.protectionLayers")}
        </h2>
        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4 text-gray-400 text-sm">
          --
        </div>
      </section>
    </div>
  );
}

function Card({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color: "green" | "gray" | "yellow";
}) {
  const dotColor = {
    green: "bg-green-500",
    gray: "bg-gray-400",
    yellow: "bg-yellow-500",
  }[color];

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
      <div className="text-xs text-gray-500 dark:text-gray-400 mb-1">{label}</div>
      <div className="flex items-center gap-2 text-sm font-medium">
        <span className={`w-2 h-2 rounded-full ${dotColor}`} />
        {value}
      </div>
    </div>
  );
}
