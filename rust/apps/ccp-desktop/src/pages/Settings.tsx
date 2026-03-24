import { useState } from "react";
import { useTranslation } from "react-i18next";
import ProfileList from "../components/ProfileList";
import ProfileForm from "../components/ProfileForm";
import Diagnostics from "../components/Diagnostics";
import ImportExport from "../components/ImportExport";

type SettingsTab = "profiles" | "diagnostics" | "global";

export default function Settings() {
  const { t, i18n } = useTranslation();
  const [tab, setTab] = useState<SettingsTab>("profiles");
  const [showCreate, setShowCreate] = useState(false);

  const toggleLanguage = () => {
    const next = i18n.language === "zh-CN" ? "en" : "zh-CN";
    i18n.changeLanguage(next);
    localStorage.setItem("ccp-lang", next);
  };

  return (
    <div>
      <h1 className="text-xl font-semibold mb-4">{t("settings.title")}</h1>

      {/* Sub-tabs */}
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

      {/* Profiles Tab */}
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

      {/* Diagnostics Tab */}
      {tab === "diagnostics" && <Diagnostics />}

      {/* Global Settings Tab */}
      {tab === "global" && (
        <div className="space-y-4">
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
            <div className="flex items-center justify-between">
              <span className="text-sm">{t("settings.language")}</span>
              <button
                onClick={toggleLanguage}
                className="px-3 py-1 text-sm bg-gray-100 dark:bg-gray-700 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600 transition-colors"
              >
                {i18n.language === "zh-CN" ? "English" : "中文"}
              </button>
            </div>
          </div>
          <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
            <div className="flex items-center justify-between">
              <span className="text-sm">{t("settings.captureMemoryLimit")}</span>
              <span className="text-sm text-gray-500">1024 MB</span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
