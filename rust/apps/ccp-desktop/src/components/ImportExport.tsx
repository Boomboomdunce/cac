import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

export default function ImportExport() {
  const { t } = useTranslation();
  const [showImport, setShowImport] = useState(false);
  const [importJson, setImportJson] = useState("");
  const [importError, setImportError] = useState<string | null>(null);

  const handleExportAll = async (exportType: string) => {
    try {
      const profiles = await invoke<{ name: string }[]>("list_profiles");
      const results: string[] = [];
      for (const p of profiles) {
        const json = await invoke<string>("export_profile", {
          name: p.name,
          exportType,
        });
        results.push(json);
      }
      const blob = new Blob([`[\n${results.join(",\n")}\n]`], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `ccp-profiles-${exportType}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      alert(String(e));
    }
  };

  const handleImport = async () => {
    setImportError(null);
    try {
      await invoke("import_profile", { jsonContent: importJson });
      setShowImport(false);
      setImportJson("");
      window.location.reload();
    } catch (e) {
      setImportError(String(e));
    }
  };

  return (
    <>
      <div className="flex gap-2">
        <button
          onClick={() => setShowImport(true)}
          className="px-3 py-1.5 text-xs bg-gray-100 dark:bg-gray-700 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600"
        >
          {t("profiles.import")}
        </button>
        <div className="relative group">
          <button className="px-3 py-1.5 text-xs bg-gray-100 dark:bg-gray-700 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600">
            {t("profiles.export")} &darr;
          </button>
          <div className="hidden group-hover:block absolute top-full left-0 mt-1 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg z-10 min-w-[140px]">
            <button
              onClick={() => handleExportAll("full")}
              className="block w-full text-left px-3 py-2 text-xs hover:bg-gray-50 dark:hover:bg-gray-700"
            >
              {t("profiles.exportFull")}
            </button>
            <button
              onClick={() => handleExportAll("redacted")}
              className="block w-full text-left px-3 py-2 text-xs hover:bg-gray-50 dark:hover:bg-gray-700"
            >
              {t("profiles.exportRedacted")}
            </button>
            <button
              onClick={() => handleExportAll("template")}
              className="block w-full text-left px-3 py-2 text-xs hover:bg-gray-50 dark:hover:bg-gray-700"
            >
              {t("profiles.exportTemplate")}
            </button>
          </div>
        </div>
      </div>

      {showImport && (
        <div className="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-gray-800 rounded-lg p-5 w-[500px] max-h-[80vh] overflow-auto shadow-xl">
            <h3 className="font-medium mb-3">{t("profiles.import")}</h3>
            {importError && (
              <div className="mb-3 p-2 bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400 rounded text-sm">
                {importError}
              </div>
            )}
            <textarea
              value={importJson}
              onChange={(e) => setImportJson(e.target.value)}
              rows={12}
              className="w-full px-3 py-2 text-xs font-mono border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 focus:ring-2 focus:ring-green-500 outline-none mb-3"
              placeholder={t("profiles.importPlaceholder")}
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => { setShowImport(false); setImportJson(""); setImportError(null); }}
                className="px-4 py-1.5 text-sm bg-gray-100 dark:bg-gray-700 rounded-lg hover:bg-gray-200"
              >
                {t("common.cancel")}
              </button>
              <button
                onClick={handleImport}
                disabled={!importJson.trim()}
                className="px-4 py-1.5 text-sm bg-green-600 text-white rounded-lg hover:bg-green-700 disabled:opacity-50"
              >
                {t("profiles.import")}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
