import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

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

interface CertInfo {
  exists: boolean;
  ca_exists: boolean;
}

interface ProxyTestResult {
  reachable: boolean;
  latency_ms: number | null;
  error: string | null;
}

export default function ProfileDetail({ name }: { name: string }) {
  const { t } = useTranslation();
  const [identity, setIdentity] = useState<ProfileIdentity | null>(null);
  const [cert, setCert] = useState<CertInfo | null>(null);
  const [proxyTest, setProxyTest] = useState<ProxyTestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [editing, setEditing] = useState(false);
  const [editProxy, setEditProxy] = useState("");
  const [editTz, setEditTz] = useState("");
  const [editLang, setEditLang] = useState("");
  const [copied, setCopied] = useState(false);

  const load = () => {
    invoke<ProfileIdentity>("get_profile_identity", { name }).then(setIdentity).catch(() => {});
    invoke<CertInfo>("get_cert_info", { name }).then(setCert).catch(() => {});
  };

  useEffect(load, [name]);

  const handleCopyAll = () => {
    if (!identity) return;
    const text = [
      `UUID: ${identity.uuid}`,
      `stable_id: ${identity.stable_id}`,
      `user_id: ${identity.user_id}`,
      `machine_id: ${identity.machine_id}`,
      `hostname: ${identity.hostname}`,
      `MAC: ${identity.mac_address}`,
      `TZ: ${identity.tz}`,
      `LANG: ${identity.lang}`,
    ].join("\n");
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleTestProxy = async () => {
    setTesting(true);
    setProxyTest(null);
    try {
      // Get profile's proxy URL from the list
      const profiles = await invoke<{ name: string; proxy_url: string | null }[]>("list_profiles");
      const p = profiles.find((pr) => pr.name === name);
      if (!p?.proxy_url) {
        setProxyTest({ reachable: false, latency_ms: null, error: "No proxy configured" });
        return;
      }
      const result = await invoke<ProxyTestResult>("test_proxy", { proxyUrl: p.proxy_url });
      setProxyTest(result);
    } catch (e) {
      setProxyTest({ reachable: false, latency_ms: null, error: String(e) });
    } finally {
      setTesting(false);
    }
  };

  const handleSaveEdit = async () => {
    try {
      await invoke("update_profile", {
        input: {
          name,
          proxy_url: editProxy || null,
          timezone: editTz || null,
          language: editLang || null,
        },
      });
      setEditing(false);
      load();
    } catch (e) {
      alert(String(e));
    }
  };

  if (!identity) return <div className="px-4 pb-4 text-sm text-gray-400">Loading...</div>;

  return (
    <div className="px-4 pb-4 border-t border-gray-100 dark:border-gray-700">
      {/* Action buttons */}
      <div className="flex items-center gap-2 mt-3 mb-2">
        <button
          onClick={handleCopyAll}
          className="px-2 py-1 text-xs bg-gray-100 dark:bg-gray-700 rounded hover:bg-gray-200 dark:hover:bg-gray-600"
        >
          {copied ? t("profiles.copied") : t("profiles.copyAll")}
        </button>
        <button
          onClick={handleTestProxy}
          disabled={testing}
          className="px-2 py-1 text-xs bg-blue-50 dark:bg-blue-900/30 text-blue-700 dark:text-blue-400 rounded hover:bg-blue-100"
        >
          {testing ? "..." : t("profiles.testProxy")}
        </button>
        <button
          onClick={() => {
            setEditing(!editing);
            setEditProxy("");
            setEditTz(identity.tz);
            setEditLang(identity.lang);
          }}
          className="px-2 py-1 text-xs bg-gray-100 dark:bg-gray-700 rounded hover:bg-gray-200 dark:hover:bg-gray-600"
        >
          {editing ? t("common.cancel") : t("profiles.edit")}
        </button>
      </div>

      {/* Proxy test result */}
      {proxyTest && (
        <div
          className={`mb-2 px-3 py-1.5 rounded text-xs ${
            proxyTest.reachable
              ? "bg-green-50 dark:bg-green-900/20 text-green-700 dark:text-green-400"
              : "bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400"
          }`}
        >
          {proxyTest.reachable
            ? `${t("profiles.proxyReachable")} (${proxyTest.latency_ms}ms)`
            : `${t("profiles.proxyUnreachable")}: ${proxyTest.error}`}
        </div>
      )}

      {/* Edit form */}
      {editing && (
        <div className="mb-3 p-3 bg-gray-50 dark:bg-gray-750 rounded-lg space-y-2">
          <div>
            <label className="block text-xs text-gray-500 mb-1">{t("profiles.proxyAddress")}</label>
            <input
              value={editProxy}
              onChange={(e) => setEditProxy(e.target.value)}
              className="w-full px-2 py-1 text-xs border border-gray-300 dark:border-gray-600 rounded bg-white dark:bg-gray-700 outline-none"
              placeholder="socks5://user:pass@host:port"
            />
          </div>
          <div className="grid grid-cols-2 gap-2">
            <div>
              <label className="block text-xs text-gray-500 mb-1">{t("profiles.timezone")}</label>
              <input
                value={editTz}
                onChange={(e) => setEditTz(e.target.value)}
                className="w-full px-2 py-1 text-xs border border-gray-300 dark:border-gray-600 rounded bg-white dark:bg-gray-700 outline-none"
              />
            </div>
            <div>
              <label className="block text-xs text-gray-500 mb-1">{t("profiles.language")}</label>
              <input
                value={editLang}
                onChange={(e) => setEditLang(e.target.value)}
                className="w-full px-2 py-1 text-xs border border-gray-300 dark:border-gray-600 rounded bg-white dark:bg-gray-700 outline-none"
              />
            </div>
          </div>
          <button
            onClick={handleSaveEdit}
            className="px-3 py-1 text-xs bg-green-600 text-white rounded hover:bg-green-700"
          >
            {t("common.save")}
          </button>
        </div>
      )}

      {/* Identity table */}
      <table className="w-full text-sm">
        <tbody>
          <Row label="UUID" value={identity.uuid} />
          <Row label="stable_id" value={identity.stable_id} />
          <Row label="user_id" value={identity.user_id} mono />
          <Row label="machine_id" value={identity.machine_id} />
          <Row label="hostname" value={identity.hostname} />
          <Row label="MAC" value={identity.mac_address} />
          <Row label="TZ" value={identity.tz} />
          <Row label="LANG" value={identity.lang} />
          <Row
            label="mTLS"
            value={cert?.exists ? t("profiles.certValid") : t("profiles.certMissing")}
          />
        </tbody>
      </table>
    </div>
  );
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <tr className="border-b border-gray-50 dark:border-gray-700/50 last:border-0">
      <td className="py-1.5 pr-4 text-gray-500 dark:text-gray-400 w-28 align-top">{label}</td>
      <td className={`py-1.5 break-all ${mono ? "font-mono text-xs" : ""}`}>{value}</td>
    </tr>
  );
}
