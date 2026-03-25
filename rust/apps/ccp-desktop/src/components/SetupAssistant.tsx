import { FormEvent, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

interface SetupStatus {
  state_root: string;
  wrappers_installed: boolean;
  can_auto_install_wrappers: boolean;
  install_metadata_present: boolean;
  install_command: string;
  suggested_bin_dir: string;
  suggested_shell_rc: string | null;
  profiles: string[];
  active_profile: string | null;
  active_profile_has_proxy: boolean;
  proxy_required_for_capture: boolean;
  capture_backend_mode: string;
  transparent_capture_available: boolean;
  transparent_capture_status: string;
  mitm_ready: boolean;
  mitm_status: string;
  mitm_system_trust_supported: boolean;
  mitm_system_trust_installed: boolean;
  mitm_system_trust_status: string;
}

export default function SetupAssistant() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<SetupStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [profileName, setProfileName] = useState("work");
  const [profileProxy, setProfileProxy] = useState("");
  const [activeProxy, setActiveProxy] = useState("");

  const refresh = async () => {
    try {
      setLoading(true);
      const next = await invoke<SetupStatus>("get_setup_status");
      setStatus(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  if (loading || !status) return null;

  const issues: string[] = [];
  if (!status.wrappers_installed) issues.push("wrappers");
  if (status.profiles.length === 0) issues.push("profiles");
  if (status.profiles.length > 0 && !status.active_profile) issues.push("active");
  if ((status.capture_backend_mode === "auto" || status.capture_backend_mode === "transparent") && !status.transparent_capture_available) {
    issues.push("transparent");
  }
  if (status.active_profile && !status.active_profile_has_proxy && status.proxy_required_for_capture) {
    issues.push("proxy");
  }
  if (!status.mitm_ready) issues.push("mitm");

  if (issues.length === 0) return null;

  const installWrappers = async () => {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await invoke("install_wrappers", { input: { updateShellRc: Boolean(status.suggested_shell_rc) } });
      setMessage(t("setup.installWrappersSuccess"));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const createAndActivateProfile = async (e: FormEvent) => {
    e.preventDefault();
    if (!profileName.trim()) return;

    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await invoke("create_profile", {
        input: {
          name: profileName.trim(),
          adapter: "claude",
          proxy_url: profileProxy.trim() ? profileProxy.trim() : null,
          timezone: null,
          language: null,
        },
      });
      await invoke("switch_profile", { name: profileName.trim() });
      setProfileProxy("");
      setMessage(t("setup.profileCreatedSuccess", { name: profileName.trim() }));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const activateProfile = async (name: string) => {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await invoke("switch_profile", { name });
      setMessage(t("setup.profileActivatedSuccess", { name }));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const saveActiveProxy = async (e: FormEvent) => {
    e.preventDefault();
    if (!status.active_profile || !activeProxy.trim()) return;

    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await invoke("update_profile", {
        input: {
          name: status.active_profile,
          proxy_url: activeProxy.trim(),
          timezone: null,
          language: null,
        },
      });
      setActiveProxy("");
      setMessage(t("setup.proxySavedSuccess", { name: status.active_profile }));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const prepareMitm = async () => {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await invoke("prepare_mitm_capture");
      setMessage(t("setup.prepareMitmSuccess"));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const installMitmTrust = async () => {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const nextMessage = await invoke<string>("install_mitm_trust");
      setMessage(nextMessage);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const removeMitmTrust = async () => {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const nextMessage = await invoke<string>("remove_mitm_trust");
      setMessage(nextMessage);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="mb-6 rounded-2xl border border-amber-300 bg-amber-50 text-amber-950 shadow-sm">
      <div className="border-b border-amber-200 px-5 py-4">
        <div className="flex items-center justify-between gap-4">
          <div>
            <h2 className="text-base font-semibold">{t("setup.title")}</h2>
            <p className="mt-1 text-sm text-amber-800">{t("setup.description")}</p>
          </div>
          <button
            onClick={refresh}
            disabled={busy}
            className="rounded-lg bg-white px-3 py-1.5 text-xs font-medium text-amber-900 hover:bg-amber-100 disabled:opacity-50"
          >
            {t("dashboard.refresh")}
          </button>
        </div>
        <p className="mt-3 text-xs text-amber-700">
          {t("setup.stateRoot")}: <span className="font-mono">{status.state_root}</span>
        </p>
      </div>

      <div className="space-y-4 px-5 py-4">
        {error && (
          <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
            {error}
          </div>
        )}

        {message && (
          <div className="rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-800">
            {message}
          </div>
        )}

        {!status.wrappers_installed && (
          <IssueCard title={t("setup.wrappersTitle")} description={t("setup.wrappersDesc")}>
            {status.can_auto_install_wrappers ? (
              <button
                onClick={installWrappers}
                disabled={busy}
                className="rounded-lg bg-amber-900 px-3 py-2 text-sm font-medium text-white hover:bg-amber-950 disabled:opacity-50"
              >
                {busy ? "..." : t("setup.installWrappers")}
              </button>
            ) : (
              <div className="space-y-2 text-sm">
                <p>{t("setup.manualInstall")}</p>
                <code className="block rounded-lg bg-white px-3 py-2 font-mono text-xs">
                  {status.install_command}
                </code>
              </div>
            )}
          </IssueCard>
        )}

        {status.profiles.length === 0 && (
          <IssueCard title={t("setup.profileTitle")} description={t("setup.profileDesc")}>
            <form onSubmit={createAndActivateProfile} className="grid gap-3 md:grid-cols-[180px_minmax(0,1fr)_auto]">
              <input
                value={profileName}
                onChange={(e) => setProfileName(e.target.value)}
                className="rounded-lg border border-amber-200 bg-white px-3 py-2 text-sm outline-none"
                placeholder={t("profiles.name")}
              />
              <input
                value={profileProxy}
                onChange={(e) => setProfileProxy(e.target.value)}
                className="rounded-lg border border-amber-200 bg-white px-3 py-2 text-sm outline-none"
                placeholder={t("setup.proxyPlaceholder")}
              />
              <button
                type="submit"
                disabled={busy || !profileName.trim()}
                className="rounded-lg bg-amber-900 px-3 py-2 text-sm font-medium text-white hover:bg-amber-950 disabled:opacity-50"
              >
                {busy ? "..." : t("setup.createAndActivate")}
              </button>
            </form>
            <p className="mt-3 text-xs text-amber-700">
              {status.proxy_required_for_capture ? t("setup.proxyHelp") : t("setup.proxyOptionalHelp")}
            </p>
          </IssueCard>
        )}

        {(status.capture_backend_mode === "auto" || status.capture_backend_mode === "transparent") && !status.transparent_capture_available && (
          <IssueCard title={t("setup.transparentTitle")} description={t("setup.transparentDesc")}>
            <div className="space-y-2 text-sm text-amber-900">
              <p>{status.transparent_capture_status}</p>
              <p className="text-xs text-amber-700">{t("setup.transparentHelp")}</p>
            </div>
          </IssueCard>
        )}

        {status.profiles.length > 0 && !status.active_profile && (
          <IssueCard title={t("setup.activeTitle")} description={t("setup.activeDesc")}>
            <div className="flex flex-wrap gap-2">
              {status.profiles.map((name) => (
                <button
                  key={name}
                  onClick={() => activateProfile(name)}
                  disabled={busy}
                  className="rounded-lg bg-white px-3 py-2 text-sm font-medium text-amber-900 hover:bg-amber-100 disabled:opacity-50"
                >
                  {t("setup.activateProfile", { name })}
                </button>
              ))}
            </div>
          </IssueCard>
        )}

        {status.active_profile && !status.active_profile_has_proxy && status.proxy_required_for_capture && (
          <IssueCard
            title={t("setup.proxyTitle", { name: status.active_profile })}
            description={t("setup.proxyDesc")}
          >
            <form onSubmit={saveActiveProxy} className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto]">
              <input
                value={activeProxy}
                onChange={(e) => setActiveProxy(e.target.value)}
                className="rounded-lg border border-amber-200 bg-white px-3 py-2 text-sm outline-none"
                placeholder={t("setup.proxyPlaceholder")}
              />
              <button
                type="submit"
                disabled={busy || !activeProxy.trim()}
                className="rounded-lg bg-amber-900 px-3 py-2 text-sm font-medium text-white hover:bg-amber-950 disabled:opacity-50"
              >
                {busy ? "..." : t("setup.saveProxy")}
              </button>
            </form>
            <p className="mt-3 text-xs text-amber-700">{t("setup.proxyHelp")}</p>
          </IssueCard>
        )}

        {status.active_profile && !status.active_profile_has_proxy && !status.proxy_required_for_capture && (
          <IssueCard
            title={t("setup.proxyTitle", { name: status.active_profile })}
            description={t("setup.proxyOptionalDesc")}
          >
            <div className="space-y-2 text-sm text-amber-900">
              <p>{t("setup.transparentStatus")}: {status.transparent_capture_status}</p>
              <p className="text-xs text-amber-700">{t("setup.proxyOptionalHelp")}</p>
            </div>
          </IssueCard>
        )}

        {!status.mitm_ready && (
          <IssueCard title={t("setup.mitmTitle")} description={t("setup.mitmDesc")}>
            <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
              <p className="text-sm text-amber-900">{status.mitm_status}</p>
              <button
                onClick={prepareMitm}
                disabled={busy}
                className="rounded-lg bg-amber-900 px-3 py-2 text-sm font-medium text-white hover:bg-amber-950 disabled:opacity-50"
              >
                {busy ? "..." : t("setup.prepareMitm")}
              </button>
            </div>
          </IssueCard>
        )}

        {status.mitm_system_trust_supported && (
          <IssueCard title={t("setup.mitmTrustTitle")} description={t("setup.mitmTrustDesc")}>
            <div className="space-y-3">
              <p className="text-sm text-amber-900">{status.mitm_system_trust_status}</p>
              {!status.mitm_ready && (
                <p className="text-xs text-amber-700">{t("setup.mitmTrustAutoPrepare")}</p>
              )}
              <div className="flex flex-wrap gap-2">
                {!status.mitm_system_trust_installed ? (
                  <button
                    onClick={installMitmTrust}
                    disabled={busy}
                    className="rounded-lg bg-amber-900 px-3 py-2 text-sm font-medium text-white hover:bg-amber-950 disabled:opacity-50"
                  >
                    {busy
                      ? "..."
                      : status.mitm_ready
                        ? t("setup.installMitmTrust")
                        : t("setup.prepareAndInstallMitmTrust")}
                  </button>
                ) : (
                  <button
                    onClick={removeMitmTrust}
                    disabled={busy}
                    className="rounded-lg bg-white px-3 py-2 text-sm font-medium text-amber-900 ring-1 ring-amber-300 hover:bg-amber-100 disabled:opacity-50"
                  >
                    {busy ? "..." : t("setup.removeMitmTrust")}
                  </button>
                )}
              </div>
            </div>
          </IssueCard>
        )}
      </div>
    </section>
  );
}

function IssueCard({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-xl border border-amber-200 bg-white px-4 py-3">
      <h3 className="text-sm font-semibold">{title}</h3>
      <p className="mt-1 text-sm text-amber-800">{description}</p>
      <div className="mt-3">{children}</div>
    </div>
  );
}
