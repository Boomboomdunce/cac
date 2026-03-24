import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import ProfileList from "../components/ProfileList";
import ProfileForm from "../components/ProfileForm";
import Diagnostics from "../components/Diagnostics";
import ImportExport from "../components/ImportExport";

type SettingsTab = "profiles" | "diagnostics" | "global";

interface GlobalSettings {
  capture_memory_limit_mb: number;
  auto_start: boolean;
  log_level: string;
  language: string;
}

export default function Settings() {
  const { t, i18n } = useTranslation();
  const [tab, setTab] = useState<SettingsTab>("profiles");
  const [showCreate, setShowCreate] = useState(false);
  const [settings, setSettings] = useState<GlobalSettings | null>(null);

  useEffect(() => {
    invoke<GlobalSettings>("get_global_settings").then(setSettings).catch(() => {});
  }, []);

  const saveSettings = async (updated: GlobalSettings) => {
    setSettings(updated);
    await invoke("save_global_settings", { settings: updated }).catch(() => {});
  };

  const changeLanguage = (lang: string) => {
    i18n.changeLanguage(lang);
    localStorage.setItem("ccp-lang", lang);
    if (settings) saveSettings({ ...settings, language: lang });
  };

  return (
    <div>
      <h1 className="text-xl font-semibold mb-4">{t("settings.title")}</h1>

      <div className="flex gap-1 mb-6 border-b border-gray-200 dark:border-gray-700">
        {(["profiles", "diagnostics", "global"] as const).map((key) => (
          <button
            key={key}
            onClick={() => setTab(key)}
            className={`px-4 py-2 text-sm transition-colors border-b-2 -mb-px ${
              tab === key
                ? "border-green-600 text-green-700 dark:text-green-400"
                : "border-transparent text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
            }`}
          >
            {t(`settings.${key}`)}
          </button>
        ))}
      </div>

      {tab === "profiles" && (
        <div>
          <div className="flex items-center gap-3 mb-4">
            <button
              onClick={() => setShowCreate(true)}
              className="px-4 py-1.5 bg-green-600 text-white text-sm rounded-lg hover:bg-green-700 transition-colors"
            >
              + {t("profiles.create")}
            </button>
            <ImportExport />
          </div>
          {showCreate && (
            <ProfileForm
              onClose={() => setShowCreate(false)}
              onCreated={() => setShowCreate(false)}
            />
          )}
          <ProfileList />
        </div>
      )}

      {tab === "diagnostics" && <Diagnostics />}

      {tab === "global" && settings && (
        <div className="space-y-4">
          {/* Language */}
          <SettingCard>
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm">{t("settings.language")}</div>
                <div className="text-xs text-gray-400 mt-0.5">{t("settings.languageDesc")}</div>
              </div>
              <select
                value={i18n.language}
                onChange={(e) => changeLanguage(e.target.value)}
                className="px-3 py-1 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 outline-none"
              >
                <option value="zh-CN">简体中文</option>
                <option value="en">English</option>
              </select>
            </div>
          </SettingCard>

          {/* Capture Memory Limit */}
          <SettingCard>
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm">{t("settings.captureMemoryLimit")}</div>
                <div className="text-xs text-gray-400 mt-0.5">{t("settings.captureMemoryDesc")}</div>
              </div>
              <select
                value={settings.capture_memory_limit_mb}
                onChange={(e) =>
                  saveSettings({ ...settings, capture_memory_limit_mb: Number(e.target.value) })
                }
                className="px-3 py-1 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 outline-none"
              >
                {[64, 128, 256, 512, 1024, 2048].map((v) => (
                  <option key={v} value={v}>
                    {v} MB
                  </option>
                ))}
              </select>
            </div>
          </SettingCard>

          {/* Auto Start */}
          <SettingCard>
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm">{t("settings.autoStart")}</div>
                <div className="text-xs text-gray-400 mt-0.5">{t("settings.autoStartDesc")}</div>
              </div>
              <button
                onClick={() => saveSettings({ ...settings, auto_start: !settings.auto_start })}
                className={`w-11 h-6 rounded-full relative transition-colors ${
                  settings.auto_start ? "bg-green-500" : "bg-gray-300 dark:bg-gray-600"
                }`}
              >
                <span
                  className={`block w-4 h-4 bg-white rounded-full absolute top-1 transition-transform ${
                    settings.auto_start ? "translate-x-6" : "translate-x-1"
                  }`}
                />
              </button>
            </div>
          </SettingCard>

          {/* Log Level */}
          <SettingCard>
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm">{t("settings.logLevel")}</div>
                <div className="text-xs text-gray-400 mt-0.5">{t("settings.logLevelDesc")}</div>
              </div>
              <select
                value={settings.log_level}
                onChange={(e) => saveSettings({ ...settings, log_level: e.target.value })}
                className="px-3 py-1 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 outline-none"
              >
                <option value="error">Error</option>
                <option value="warn">Warn</option>
                <option value="info">Info</option>
                <option value="debug">Debug</option>
              </select>
            </div>
          </SettingCard>

          {/* State Root */}
          <SettingCard>
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm">{t("settings.stateRoot")}</div>
                <div className="text-xs text-gray-400 mt-0.5 font-mono">~/.ccp-rust/</div>
              </div>
            </div>
          </SettingCard>
        </div>
      )}
    </div>
  );
}

function SettingCard({ children }: { children: React.ReactNode }) {
  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
      {children}
    </div>
  );
}
