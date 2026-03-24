import { useTranslation } from "react-i18next";

export default function Settings() {
  const { t, i18n } = useTranslation();

  const toggleLanguage = () => {
    const next = i18n.language === "zh-CN" ? "en" : "zh-CN";
    i18n.changeLanguage(next);
    localStorage.setItem("ccp-lang", next);
  };

  return (
    <div>
      <h1 className="text-xl font-semibold mb-6">{t("settings.title")}</h1>

      {/* Language Switcher */}
      <section className="mb-8">
        <h2 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-3">
          {t("settings.global")}
        </h2>
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
      </section>

      {/* Profiles Placeholder */}
      <section className="mb-8">
        <h2 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-3">
          {t("settings.profiles")}
        </h2>
        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-8 text-center text-gray-400 text-sm">
          --
        </div>
      </section>

      {/* Diagnostics Placeholder */}
      <section>
        <h2 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-3">
          {t("settings.diagnostics")}
        </h2>
        <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-8 text-center text-gray-400 text-sm">
          --
        </div>
      </section>
    </div>
  );
}
