import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

interface AppStatus {
  active: boolean;
  paused: boolean;
  profile: string | null;
  version: string;
}

interface ProfileIdentity {
  uuid: string;
  stable_id: string;
  user_id: string;
  machine_id: string;
  hostname: string;
  mac_address: string;
  tz: string;
  lang: string;
}

export default function Dashboard() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [identity, setIdentity] = useState<ProfileIdentity | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<AppStatus>("get_status")
      .then(setStatus)
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    if (status?.profile) {
      invoke<ProfileIdentity>("get_profile_identity", { name: status.profile })
        .then(setIdentity)
        .catch(() => setIdentity(null));
    } else {
      setIdentity(null);
    }
  }, [status?.profile]);

  const statusColor = status?.active ? "green" : status?.paused ? "yellow" : "gray";
  const statusLabel = status?.active
    ? t("status.running")
    : status?.paused
      ? t("status.stopped")
      : t("status.stopped");

  return (
    <div>
      <h1 className="text-xl font-semibold mb-6">{t("dashboard.title")}</h1>

      {error && (
        <div className="mb-4 p-3 bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400 rounded-lg text-sm">
          {error}
        </div>
      )}

      {/* Status Cards */}
      <div className="grid grid-cols-4 gap-4 mb-8">
        <Card label={t("status.protection")} value={statusLabel} color={statusColor} />
        <Card
          label={t("status.profile")}
          value={status?.profile ?? t("status.noProfile")}
          color={status?.profile ? "green" : "gray"}
        />
        <Card label={t("status.egressIp")} value="--" color="gray" />
        <Card label="Version" value={status?.version ?? "--"} color="gray" />
      </div>

      {/* Current Profile Identity */}
      {identity && (
        <section className="mb-8">
          <h2 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-3">
            {t("dashboard.proxiedTools")}
          </h2>
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700">
            <button
              onClick={() => setExpanded(!expanded)}
              className="w-full flex items-center justify-between px-4 py-3 text-sm hover:bg-gray-50 dark:hover:bg-gray-750 transition-colors"
            >
              <div className="flex items-center gap-3">
                <span className="w-2 h-2 rounded-full bg-gray-400" />
                <span className="font-medium">claude</span>
                <span className="text-gray-400">({t("status.profile")}: {status?.profile})</span>
              </div>
              <svg
                className={`w-4 h-4 text-gray-400 transition-transform ${expanded ? "rotate-180" : ""}`}
                viewBox="0 0 24 24"
                fill="currentColor"
              >
                <path d="M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6z" />
              </svg>
            </button>

            {expanded && (
              <div className="px-4 pb-4 border-t border-gray-100 dark:border-gray-700">
                <table className="w-full text-sm mt-3">
                  <tbody>
                    <IdentityRow label="UUID" value={identity.uuid} />
                    <IdentityRow label="stable_id" value={identity.stable_id} />
                    <IdentityRow label="user_id" value={identity.user_id} mono />
                    <IdentityRow label="machine_id" value={identity.machine_id} />
                    <IdentityRow label="hostname" value={identity.hostname} />
                    <IdentityRow label="MAC" value={identity.mac_address} />
                    <IdentityRow label="TZ" value={identity.tz} />
                    <IdentityRow label="LANG" value={identity.lang} />
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </section>
      )}

      {!identity && (
        <section className="mb-8">
          <h2 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-3">
            {t("dashboard.proxiedTools")}
          </h2>
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-8 text-center text-gray-400 text-sm">
            {t("dashboard.noToolsRunning")}
          </div>
        </section>
      )}

      {/* Protection Layers */}
      <section>
        <h2 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-3">
          {t("dashboard.protectionLayers")}
        </h2>
        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4 space-y-2 text-sm">
          <ProtectionRow
            ok={status?.active ?? false}
            label={t("dashboard.protectionLayers")}
            desc={status?.active ? "Active" : "Inactive"}
          />
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

function IdentityRow({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <tr className="border-b border-gray-50 dark:border-gray-700/50 last:border-0">
      <td className="py-1.5 pr-4 text-gray-500 dark:text-gray-400 w-28">{label}</td>
      <td className={`py-1.5 ${mono ? "font-mono text-xs" : ""} break-all`}>{value}</td>
    </tr>
  );
}

function ProtectionRow({
  ok,
  label,
  desc,
}: {
  ok: boolean;
  label: string;
  desc: string;
}) {
  return (
    <div className="flex items-center gap-2">
      <span className={ok ? "text-green-500" : "text-gray-400"}>
        {ok ? "\u2705" : "\u26AA"}
      </span>
      <span>{label}</span>
      <span className="text-gray-400 ml-auto">{desc}</span>
    </div>
  );
}
