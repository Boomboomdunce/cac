import { useState } from "react";
import Sidebar from "./components/Sidebar";
import SetupAssistant from "./components/SetupAssistant";
import StatusBar from "./components/StatusBar";
import Dashboard from "./pages/Dashboard";
import TrafficCapture from "./pages/TrafficCapture";
import Settings from "./pages/Settings";

type Page = "dashboard" | "traffic" | "settings";

function App() {
  const [page, setPage] = useState<Page>("dashboard");

  return (
    <div className="flex h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100">
      <Sidebar current={page} onNavigate={setPage} />
      <div className="flex flex-col flex-1 min-w-0">
        <main className="flex-1 overflow-auto p-6">
          <SetupAssistant />
          {page === "dashboard" && <Dashboard />}
          {page === "traffic" && <TrafficCapture />}
          {page === "settings" && <Settings />}
        </main>
        <StatusBar />
      </div>
    </div>
  );
}

export default App;
