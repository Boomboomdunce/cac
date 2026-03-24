import { useTranslation } from "react-i18next";

type Page = "dashboard" | "traffic" | "settings";

interface SidebarProps {
  current: Page;
  onNavigate: (page: Page) => void;
}

const navItems: { key: Page; icon: string }[] = [
  { key: "dashboard", icon: "chart" },
  { key: "traffic", icon: "search" },
  { key: "settings", icon: "cog" },
];

function NavIcon({ icon }: { icon: string }) {
  const paths: Record<string, string> = {
    chart:
      "M3 13h2v8H3zm6-4h2v12H9zm6-6h2v18h-2z",
    search:
      "M15.5 14h-.79l-.28-.27A6.47 6.47 0 0 0 16 9.5 6.5 6.5 0 1 0 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z",
    cog: "M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 0 0 .12-.61l-1.92-3.32a.49.49 0 0 0-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 0 0-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.07.62-.07.94s.02.64.07.94l-2.03 1.58a.49.49 0 0 0-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6A3.6 3.6 0 1 1 12 8.4a3.6 3.6 0 0 1 0 7.2z",
  };
  return (
    <svg
      className="w-5 h-5"
      viewBox="0 0 24 24"
      fill="currentColor"
    >
      <path d={paths[icon] || ""} />
    </svg>
  );
}

export default function Sidebar({ current, onNavigate }: SidebarProps) {
  const { t } = useTranslation();

  return (
    <aside className="w-48 bg-white dark:bg-gray-800 border-r border-gray-200 dark:border-gray-700 flex flex-col">
      <div className="px-4 py-4 font-bold text-lg tracking-tight">
        CCP
      </div>
      <nav className="flex-1 px-2 space-y-1">
        {navItems.map((item) => (
          <button
            key={item.key}
            onClick={() => onNavigate(item.key)}
            className={`w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors ${
              current === item.key
                ? "bg-green-50 dark:bg-green-900/30 text-green-700 dark:text-green-400 font-medium"
                : "text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700"
            }`}
          >
            <NavIcon icon={item.icon} />
            {t(`nav.${item.key}`)}
          </button>
        ))}
      </nav>
    </aside>
  );
}
