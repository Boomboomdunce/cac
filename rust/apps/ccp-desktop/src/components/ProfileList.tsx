import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import ProfileDetail from "./ProfileDetail";

interface ProfileSummary {
  name: string;
  adapter: string;
  proxy_url: string | null;
  active: boolean;
}

export default function ProfileList() {
  const { t } = useTranslation();
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    invoke<ProfileSummary[]>("list_profiles")
      .then(setProfiles)
      .catch((e) => setError(String(e)));
  };

  useEffect(load, []);

  const handleSwitch = async (name: string) => {
    try {
      await invoke("switch_profile", { name });
      load();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (name: string) => {
    try {
      await invoke("delete_profile", { name });
      if (expanded === name) setExpanded(null);
      load();
    } catch (e) {
      setError(String(e));
    }
  };

  if (profiles.length === 0 && !error) {
    return (
      <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-8 text-center text-gray-400 text-sm">
        {t("profiles.empty")}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {error && (
        <div className="p-3 bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400 rounded-lg text-sm">
          {error}
        </div>
      )}
      {profiles.map((p) => (
        <div
          key={p.name}
          className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700"
        >
          <div className="flex items-center justify-between px-4 py-3">
            <div className="flex items-center gap-3">
              <span
                className={`w-2.5 h-2.5 rounded-full ${
                  p.active ? "bg-green-500" : "bg-gray-300 dark:bg-gray-600"
                }`}
              />
              <div>
                <span className="font-medium text-sm">{p.name}</span>
                {p.active && (
                  <span className="ml-2 text-xs text-green-600 dark:text-green-400">
                    {t("profiles.active")}
                  </span>
                )}
                <div className="text-xs text-gray-400 mt-0.5">
                  {p.adapter} &middot; {p.proxy_url ?? "--"}
                </div>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => setExpanded(expanded === p.name ? null : p.name)}
                className="px-2 py-1 text-xs bg-gray-100 dark:bg-gray-700 rounded hover:bg-gray-200 dark:hover:bg-gray-600"
              >
                {t("profiles.details")}
              </button>
              {!p.active && (
                <button
                  onClick={() => handleSwitch(p.name)}
                  className="px-2 py-1 text-xs bg-green-50 dark:bg-green-900/30 text-green-700 dark:text-green-400 rounded hover:bg-green-100 dark:hover:bg-green-900/50"
                >
                  {t("profiles.switchTo")}
                </button>
              )}
              <button
                onClick={() => handleDelete(p.name)}
                className="px-2 py-1 text-xs bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded hover:bg-red-100 dark:hover:bg-red-900/30"
              >
                {t("profiles.delete")}
              </button>
            </div>
          </div>
          {expanded === p.name && <ProfileDetail name={p.name} />}
        </div>
      ))}
    </div>
  );
}
