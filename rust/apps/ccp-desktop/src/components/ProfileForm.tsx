import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

interface Props {
  onClose: () => void;
  onCreated: () => void;
}

export default function ProfileForm({ onClose, onCreated }: Props) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [proxyUrl, setProxyUrl] = useState("");
  const [adapter, setAdapter] = useState("claude");
  const [timezone, setTimezone] = useState("");
  const [language, setLanguage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);
    try {
      await invoke("create_profile", {
        input: {
          name,
          adapter,
          proxy_url: proxyUrl || null,
          timezone: timezone || null,
          language: language || null,
        },
      });
      onCreated();
      // Force a page reload to refresh profile list
      window.location.reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-5 mb-4">
      <div className="flex items-center justify-between mb-4">
        <h3 className="font-medium">{t("profiles.create")}</h3>
        <button onClick={onClose} className="text-gray-400 hover:text-gray-600 text-lg">
          &times;
        </button>
      </div>

      {error && (
        <div className="mb-3 p-2 bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400 rounded text-sm">
          {error}
        </div>
      )}

      <form onSubmit={handleSubmit} className="space-y-3">
        <Field label={t("profiles.name")}>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
            className="w-full px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 focus:ring-2 focus:ring-green-500 outline-none"
            placeholder="us1"
          />
        </Field>

        <Field label={t("profiles.proxyAddress")}>
          <input
            value={proxyUrl}
            onChange={(e) => setProxyUrl(e.target.value)}
            className="w-full px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 focus:ring-2 focus:ring-green-500 outline-none"
            placeholder="socks5://user:pass@host:port"
          />
        </Field>

        <Field label={t("profiles.adapter")}>
          <select
            value={adapter}
            onChange={(e) => setAdapter(e.target.value)}
            className="w-full px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 focus:ring-2 focus:ring-green-500 outline-none"
          >
            <option value="claude">claude</option>
          </select>
        </Field>

        <div className="grid grid-cols-2 gap-3">
          <Field label={t("profiles.timezone")}>
            <input
              value={timezone}
              onChange={(e) => setTimezone(e.target.value)}
              className="w-full px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 focus:ring-2 focus:ring-green-500 outline-none"
              placeholder="America/New_York"
            />
          </Field>
          <Field label={t("profiles.language")}>
            <input
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
              className="w-full px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 focus:ring-2 focus:ring-green-500 outline-none"
              placeholder="en_US.UTF-8"
            />
          </Field>
        </div>

        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-1.5 text-sm bg-gray-100 dark:bg-gray-700 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600"
          >
            {t("common.cancel")}
          </button>
          <button
            type="submit"
            disabled={loading || !name}
            className="px-4 py-1.5 text-sm bg-green-600 text-white rounded-lg hover:bg-green-700 disabled:opacity-50"
          >
            {loading ? "..." : t("profiles.create")}
          </button>
        </div>
      </form>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <label className="block text-xs text-gray-500 dark:text-gray-400 mb-1">{label}</label>
      {children}
    </div>
  );
}
