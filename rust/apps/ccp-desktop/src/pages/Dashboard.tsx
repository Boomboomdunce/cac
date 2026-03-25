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

interface ProtectionLayer {
  name: string;
  active: boolean;
  description: string;
}

const LAYER_LABELS: Record<string, { zh: string; en: string }> = {
  proxy_injection: { zh: "代理注入", en: "Proxy Injection" },
  dns_telemetry_block: { zh: "DNS 遥测拦截", en: "DNS Telemetry Block" },
  env_var_protection: { zh: "环境变量保护", en: "Env Var Protection" },
  device_identity_isolation: { zh: "设备身份隔离", en: "Device Identity Isolation" },
  mtls_cert_injection: { zh: "mTLS 证书注入", en: "mTLS Cert Injection" },
  fetch_interception: { zh: "fetch 拦截", en: "Fetch Interception" },
  https_mitm_capture: { zh: "HTTPS MITM 捕获", en: "HTTPS MITM Capture" },
  system_cert_trust: { zh: "系统证书信任", en: "System Cert Trust" },
  ipv6_protection: { zh: "IPv6 防护", en: "IPv6 Protection" },
};

export default function Dashboard() {
  const { t, i18n } = useTranslation();
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [identity, setIdentity] = useState<ProfileIdentity | null>(null);
  const [layers, setLayers] = useState<ProtectionLayer[]>([]);
  const [egressIp, setEgressIp] = useState<string | null>(null);
  const [egressLoading, setEgressLoading] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    invoke<AppStatus>("get_status")
      .then(setStatus)
      .catch((e) => setError(String(e)));
    invoke<ProtectionLayer[]>("get_protection_layers")
      .then(setLayers)
      .catch(() => {});
  };

  const detectIp = () => {
    setEgressLoading(true);
    invoke<string>("detect_egress_ip")
      .then(setEgressIp)
      .catch(() => setEgressIp(null))
      .finally(() => setEgressLoading(false));
  };

  useEffect(() => {
    refresh();
    detectIp();
    const timer = setInterval(refresh, 30000);
    return () => clearInterval(timer);
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
    : t("status.stopped");

  const isZh = i18n.language.startsWith("zh");

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-xl font-semibold">{t("dashboard.title")}</h1>
        <button
          onClick={refresh}
          className="px-3 py-1 text-xs bg-gray-100 dark:bg-gray-700 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600"
        >
          {t("dashboard.refresh")}
        </button>
      </div>

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
        <Card
          label={t("status.egressIp")}
          value={egressLoading ? "..." : egressIp ?? "--"}
          color={egressIp ? "green" : "gray"}
        />
        <Card label="Version" value={status?.version ?? "--"} color="gray" />
      </div>

      {/* Current Profile Identity */}
      <section className="mb-8">
        <h2 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-3">
          {t("dashboard.proxiedTools")}
        </h2>
        {identity ? (
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700">
            <button
              onClick={() => setExpanded(!expanded)}
              className="w-full flex items-center justify-between px-4 py-3 text-sm hover:bg-gray-50 dark:hover:bg-gray-750 transition-colors"
            >
              <div className="flex items-center gap-3">
                <span className={`w-2 h-2 rounded-full ${status?.active ? "bg-green-500" : "bg-gray-400"}`} />
                <span className="font-medium">claude</span>
                <span className="text-gray-400">({status?.profile})</span>
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
                    <IdRow label="UUID" value={identity.uuid} />
                    <IdRow label="stable_id" value={identity.stable_id} />
                    <IdRow label="user_id" value={identity.user_id} mono />
                    <IdRow label="machine_id" value={identity.machine_id} />
                    <IdRow label="hostname" value={identity.hostname} />
                    <IdRow label="MAC" value={identity.mac_address} />
                    <IdRow label="TZ" value={identity.tz} />
                    <IdRow label="LANG" value={identity.lang} />
                  </tbody>
                </table>
              </div>
            )}
          </div>
        ) : (
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-8 text-center text-gray-400 text-sm">
            {t("dashboard.noToolsRunning")}
          </div>
        )}
      </section>

      {/* Protection Layers */}
      <section>
        <h2 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-3">
          {t("dashboard.protectionLayers")}
        </h2>
        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 divide-y divide-gray-50 dark:divide-gray-700/30">
          {layers.length === 0 ? (
            <div className="p-4 text-gray-400 text-sm text-center">--</div>
          ) : (
            layers.map((layer) => {
              const labels = LAYER_LABELS[layer.name];
              const label = labels ? (isZh ? labels.zh : labels.en) : layer.name;
              return (
                <div key={layer.name} className="px-4 py-2.5 flex items-center gap-3">
                  <span
                    className={`w-2.5 h-2.5 rounded-full flex-shrink-0 ${
                      layer.active
                        ? "bg-green-500"
                        : layer.name === "ipv6_protection"
                          ? "bg-yellow-500"
                          : "bg-gray-300 dark:bg-gray-600"
                    }`}
                  />
                  <span className="text-sm flex-shrink-0 w-40">{label}</span>
                  <span className="text-xs text-gray-500 dark:text-gray-400">
                    {layer.description}
                  </span>
                </div>
              );
            })
          )}
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

function IdRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <tr className="border-b border-gray-50 dark:border-gray-700/50 last:border-0">
      <td className="py-1.5 pr-4 text-gray-500 dark:text-gray-400 w-28">{label}</td>
      <td className={`py-1.5 ${mono ? "font-mono text-xs" : ""} break-all`}>{value}</td>
    </tr>
  );
}
