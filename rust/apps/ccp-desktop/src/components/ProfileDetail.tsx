import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

interface ProfileIdentity {
  uuid: string;
  stable_id: string;
  user_id: string;
  machine_id: string;
  hostname: string;
  mac_address: string;
  tz: string;
  lang: string;
}

interface CertInfo {
  exists: boolean;
  ca_exists: boolean;
}

export default function ProfileDetail({ name }: { name: string }) {
  const { t } = useTranslation();
  const [identity, setIdentity] = useState<ProfileIdentity | null>(null);
  const [cert, setCert] = useState<CertInfo | null>(null);

  useEffect(() => {
    invoke<ProfileIdentity>("get_profile_identity", { name }).then(setIdentity).catch(() => {});
    invoke<CertInfo>("get_cert_info", { name }).then(setCert).catch(() => {});
  }, [name]);

  if (!identity) return <div className="px-4 pb-4 text-sm text-gray-400">Loading...</div>;

  return (
    <div className="px-4 pb-4 border-t border-gray-100 dark:border-gray-700">
      <table className="w-full text-sm mt-3">
        <tbody>
          <Row label="UUID" value={identity.uuid} />
          <Row label="stable_id" value={identity.stable_id} />
          <Row label="user_id" value={identity.user_id} mono />
          <Row label="machine_id" value={identity.machine_id} />
          <Row label="hostname" value={identity.hostname} />
          <Row label="MAC" value={identity.mac_address} />
          <Row label="TZ" value={identity.tz} />
          <Row label="LANG" value={identity.lang} />
          <Row
            label="mTLS"
            value={
              cert?.exists
                ? t("profiles.certValid")
                : t("profiles.certMissing")
            }
          />
        </tbody>
      </table>
    </div>
  );
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <tr className="border-b border-gray-50 dark:border-gray-700/50 last:border-0">
      <td className="py-1.5 pr-4 text-gray-500 dark:text-gray-400 w-28 align-top">{label}</td>
      <td className={`py-1.5 break-all ${mono ? "font-mono text-xs" : ""}`}>{value}</td>
    </tr>
  );
}
