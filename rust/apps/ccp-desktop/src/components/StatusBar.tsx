import { useTranslation } from "react-i18next";

export default function StatusBar() {
  const { t } = useTranslation();

  return (
    <div className="h-8 px-4 flex items-center gap-4 text-xs text-gray-500 dark:text-gray-400 border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800">
      <span className="flex items-center gap-1.5">
        <span className="w-2 h-2 rounded-full bg-gray-400" />
        {t("status.stopped")}
      </span>
      <span>{t("status.noProfile")}</span>
    </div>
  );
}
