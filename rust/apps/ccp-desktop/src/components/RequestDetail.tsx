import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type { CapturedRequest } from "../pages/TrafficCapture";

type Tab = "reqHeaders" | "reqBody" | "resHeaders" | "resBody" | "privacy";

interface Props {
  request: CapturedRequest;
  onClose: () => void;
}

export default function RequestDetail({ request, onClose }: Props) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("reqHeaders");

  const isBlocked = request.category === "blocked";

  const tabs: { key: Tab; label: string }[] = [
    { key: "reqHeaders", label: t("detail.requestHeaders") },
    { key: "reqBody", label: t("detail.requestBody") },
    { key: "resHeaders", label: t("detail.responseHeaders") },
    { key: "resBody", label: t("detail.responseBody") },
    { key: "privacy", label: t("detail.privacyComparison") },
  ];

  return (
    <div className="mt-3 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 max-h-[320px] flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-gray-100 dark:border-gray-700">
        <div className="flex items-center gap-2 text-sm">
          <span className="font-medium">#{request.id}</span>
          <span className={isBlocked ? "text-red-500" : "text-gray-600 dark:text-gray-400"}>
            {isBlocked ? "BLOCKED" : `${request.method} ${request.status ?? ""}`}
          </span>
          <span className="text-xs text-gray-400 truncate max-w-[400px]">{request.url}</span>
        </div>
        <button onClick={onClose} className="text-gray-400 hover:text-gray-600 text-lg ml-2">
          &times;
        </button>
      </div>

      {/* Blocked detail */}
      {isBlocked && (
        <div className="px-4 py-3 bg-red-50 dark:bg-red-900/10 text-sm">
          <div className="text-red-700 dark:text-red-400 font-medium mb-1">
            {t("detail.blockedTitle")}
          </div>
          <div className="text-xs text-red-600 dark:text-red-400/80">
            {request.blocked_reason ?? t("detail.blockedDefault")}
          </div>
        </div>
      )}

      {/* Tabs */}
      {!isBlocked && (
        <>
          <div className="flex gap-0 border-b border-gray-100 dark:border-gray-700 px-2">
            {tabs.map((t) => (
              <button
                key={t.key}
                onClick={() => setTab(t.key)}
                className={`px-3 py-1.5 text-xs border-b-2 -mb-px transition-colors ${
                  tab === t.key
                    ? "border-green-600 text-green-700 dark:text-green-400"
                    : "border-transparent text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
                }`}
              >
                {t.label}
              </button>
            ))}
          </div>

          <div className="flex-1 overflow-auto p-4 text-xs font-mono whitespace-pre-wrap">
            {tab === "reqHeaders" && <HeadersView headers={request.request_headers} />}
            {tab === "reqBody" && <BodyView body={request.request_body} />}
            {tab === "resHeaders" && <HeadersView headers={request.response_headers} />}
            {tab === "resBody" && <BodyView body={request.response_body} />}
            {tab === "privacy" && <PrivacyComparison request={request} />}
          </div>
        </>
      )}
    </div>
  );
}

function HeadersView({ headers }: { headers: [string, string][] }) {
  const { t } = useTranslation();
  if (!headers || headers.length === 0) {
    return <span className="text-gray-400">{t("detail.noData")}</span>;
  }
  return (
    <table className="w-full">
      <tbody>
        {headers.map(([key, val], i) => (
          <tr key={i} className="border-b border-gray-50 dark:border-gray-700/30 last:border-0">
            <td className="pr-4 py-0.5 text-green-700 dark:text-green-400 whitespace-nowrap align-top">
              {key}
            </td>
            <td className="py-0.5 break-all text-gray-600 dark:text-gray-300">
              {key.toLowerCase() === "authorization" ? maskAuth(val) : val}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function BodyView({ body }: { body: string | null }) {
  const { t } = useTranslation();
  if (!body) return <span className="text-gray-400">{t("detail.noData")}</span>;

  // Try to pretty-print JSON
  try {
    const parsed = JSON.parse(body);
    return <pre className="text-gray-700 dark:text-gray-300">{JSON.stringify(parsed, null, 2)}</pre>;
  } catch {
    return <pre className="text-gray-700 dark:text-gray-300">{body}</pre>;
  }
}

function PrivacyComparison({ request: _request }: { request: CapturedRequest }) {
  const { t } = useTranslation();
  const [device, setDevice] = useState<{ hostname: string } | null>(null);
  const [identity, setIdentity] = useState<{
    uuid: string;
    hostname: string;
    mac_address: string;
    tz: string;
  } | null>(null);

  useEffect(() => {
    invoke<{ hostname: string; uuid: string }>("get_device_identity").then(setDevice).catch(() => {});
    invoke<{ profile: string | null }>("get_status").then(async (s) => {
      if (s.profile) {
        const id = await invoke<{
          uuid: string;
          hostname: string;
          mac_address: string;
          tz: string;
        }>("get_profile_identity", { name: s.profile });
        setIdentity(id);
      }
    }).catch(() => {});
  }, []);

  return (
    <div className="text-sm">
      <p className="text-gray-500 mb-3 font-sans">{t("detail.privacyDesc")}</p>
      <table className="w-full font-sans">
        <thead>
          <tr className="border-b border-gray-200 dark:border-gray-700">
            <th className="text-left py-1.5 text-xs text-gray-500 w-32">{t("detail.field")}</th>
            <th className="text-left py-1.5 text-xs text-gray-500">{t("detail.realValue")}</th>
            <th className="text-left py-1.5 text-xs text-gray-500">{t("detail.protectedValue")}</th>
          </tr>
        </thead>
        <tbody className="text-xs">
          <ComparisonRow field="Machine UUID" real={device?.hostname ? "(system)" : "--"} replaced={identity?.uuid ?? "--"} />
          <ComparisonRow field="hostname" real={device?.hostname ?? "--"} replaced={identity?.hostname ?? "--"} />
          <ComparisonRow field="MAC Address" real="(system)" replaced={identity?.mac_address ?? "--"} />
          <ComparisonRow field="Timezone" real={Intl.DateTimeFormat().resolvedOptions().timeZone} replaced={identity?.tz ?? "--"} />
          <ComparisonRow field="Egress IP" real="(direct IP)" replaced="(proxy IP)" />
        </tbody>
      </table>
      {identity && (
        <div className="mt-3 text-xs text-green-600 dark:text-green-400 font-sans">
          {t("detail.fieldsReplaced", { count: 5 })}
        </div>
      )}
    </div>
  );
}

function ComparisonRow({
  field,
  real,
  replaced,
}: {
  field: string;
  real: string;
  replaced: string;
}) {
  return (
    <tr className="border-b border-gray-50 dark:border-gray-700/30">
      <td className="py-1.5 text-gray-500">{field}</td>
      <td className="py-1.5 text-gray-400">{real}</td>
      <td className="py-1.5 text-green-700 dark:text-green-400">{replaced}</td>
    </tr>
  );
}

function maskAuth(value: string): string {
  if (value.length <= 10) return "***";
  return value.slice(0, 7) + "***..." + value.slice(-3);
}
