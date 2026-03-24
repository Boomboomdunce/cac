use crate::{error::StoreError, layout::StateLayout, secret_store::create_secure_file};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct RuntimeShimSet {
    pub dir: PathBuf,
}

pub fn ensure_runtime_shims(layout: &StateLayout) -> Result<RuntimeShimSet, StoreError> {
    let dir = layout.hooks_dir().join("shim-bin");
    fs::create_dir_all(&dir)?;

    #[cfg(unix)]
    {
        write_executable(&dir.join("hostname"), unix_hostname_shim())?;
        write_executable(&dir.join("cat"), unix_cat_shim())?;
        write_executable(&dir.join("ioreg"), unix_ioreg_shim())?;
        write_executable(&dir.join("ifconfig"), unix_ifconfig_shim())?;
    }

    #[cfg(windows)]
    {
        write_text(&dir.join("hostname.cmd"), windows_hostname_shim())?;
        write_text(&dir.join("getmac.cmd"), windows_getmac_shim())?;
        write_text(&dir.join("wmic.cmd"), windows_wmic_shim())?;
        write_text(&dir.join("reg.cmd"), windows_reg_shim())?;
        write_text(&dir.join("powershell.cmd"), windows_powershell_shim())?;
    }

    Ok(RuntimeShimSet { dir })
}

fn unix_hostname_shim() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail
if [[ -n "${CCP_FAKE_HOSTNAME:-}" ]]; then
  printf '%s\n' "$CCP_FAKE_HOSTNAME"
  exit 0
fi
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REAL_PATH=$(printf '%s' "${PATH:-}" | tr ':' '\n' | grep -Fxv "$SCRIPT_DIR" | paste -sd ':' -)
REAL_BIN=$(PATH="$REAL_PATH" command -v hostname || true)
[[ -n "$REAL_BIN" ]] && exec "$REAL_BIN" "$@"
exit 1
"#
}

fn unix_cat_shim() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "/etc/machine-id" ]] || [[ "${1:-}" == "/var/lib/dbus/machine-id" ]]; then
  if [[ -n "${CCP_FAKE_MACHINE_ID:-}" ]]; then
    printf '%s\n' "$CCP_FAKE_MACHINE_ID"
    exit 0
  fi
fi
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REAL_PATH=$(printf '%s' "${PATH:-}" | tr ':' '\n' | grep -Fxv "$SCRIPT_DIR" | paste -sd ':' -)
REAL_BIN=$(PATH="$REAL_PATH" command -v cat || true)
[[ -n "$REAL_BIN" ]] && exec "$REAL_BIN" "$@"
exit 1
"#
}

fn unix_ioreg_shim() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$*" == *"IOPlatformExpertDevice"* ]] && [[ -n "${CCP_FAKE_PLATFORM_UUID:-}" ]]; then
  cat <<EOF
+-o Root  <class IORegistryEntry, id 0x100000100, retain 11>
  +-o J314sAP  <class IOPlatformExpertDevice, id 0x100000101, registered, matched, active, busy 0 (0 ms), retain 28>
    {
      "IOPlatformUUID" = "${CCP_FAKE_PLATFORM_UUID}"
      "IOPlatformSerialNumber" = "C02FAKE000001"
      "manufacturer" = "Apple Inc."
      "model" = "Mac14,5"
    }
EOF
  exit 0
fi
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REAL_PATH=$(printf '%s' "${PATH:-}" | tr ':' '\n' | grep -Fxv "$SCRIPT_DIR" | paste -sd ':' -)
REAL_BIN=$(PATH="$REAL_PATH" command -v ioreg || true)
[[ -n "$REAL_BIN" ]] && exec "$REAL_BIN" "$@"
exit 0
"#
}

fn unix_ifconfig_shim() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REAL_PATH=$(printf '%s' "${PATH:-}" | tr ':' '\n' | grep -Fxv "$SCRIPT_DIR" | paste -sd ':' -)
REAL_BIN=$(PATH="$REAL_PATH" command -v ifconfig || true)
[[ -n "$REAL_BIN" ]] || exit 1
if [[ -n "${CCP_FAKE_MAC_ADDRESS:-}" ]]; then
  "$REAL_BIN" "$@" | sed -E "s/ether ([0-9a-f]{2}:){5}[0-9a-f]{2}/ether ${CCP_FAKE_MAC_ADDRESS}/g"
  exit 0
fi
exec "$REAL_BIN" "$@"
"#
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_hostname_shim() -> &'static str {
    "@echo off\r\nsetlocal\r\nif not \"%CCP_FAKE_HOSTNAME%\"==\"\" (\r\n  echo %CCP_FAKE_HOSTNAME%\r\n  exit /b 0\r\n)\r\n\"%SystemRoot%\\System32\\hostname.exe\" %*\r\n"
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_getmac_shim() -> &'static str {
    "@echo off\r\nsetlocal EnableDelayedExpansion\r\nif not \"%CCP_FAKE_MAC_ADDRESS%\"==\"\" (\r\n  set \"MAC=%CCP_FAKE_MAC_ADDRESS::=-%\"\r\n  echo Physical Address    Transport Name\r\n  echo =================== ==========================================================\r\n  echo !MAC!               \\Device\\Tcpip_{CCP-FAKE-ADAPTER}\r\n  exit /b 0\r\n)\r\n\"%SystemRoot%\\System32\\getmac.exe\" %*\r\n"
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_wmic_shim() -> &'static str {
    "@echo off\r\nsetlocal EnableDelayedExpansion\r\nset \"ARGS=%*\"\r\nif not \"!ARGS:csproduct=!\"==\"!ARGS!\" if not \"!ARGS:UUID=!\"==\"!ARGS!\" if not \"%CCP_FAKE_PLATFORM_UUID%\"==\"\" (\r\n  echo UUID\r\n  echo %CCP_FAKE_PLATFORM_UUID%\r\n  exit /b 0\r\n)\r\nif not \"!ARGS:nic=!\"==\"!ARGS!\" if not \"!ARGS:MACAddress=!\"==\"!ARGS!\" if not \"%CCP_FAKE_MAC_ADDRESS%\"==\"\" (\r\n  set \"MAC=%CCP_FAKE_MAC_ADDRESS::=-%\"\r\n  echo MACAddress\r\n  echo !MAC!\r\n  exit /b 0\r\n)\r\n\"%SystemRoot%\\System32\\wbem\\wmic.exe\" %*\r\n"
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_reg_shim() -> &'static str {
    "@echo off\r\nsetlocal EnableDelayedExpansion\r\nset \"ARGS=%*\"\r\nif not \"!ARGS:MachineGuid=!\"==\"!ARGS!\" if not \"!ARGS:Microsoft\\Cryptography=!\"==\"!ARGS!\" if not \"%CCP_FAKE_MACHINE_ID%\"==\"\" (\r\n  echo HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography\r\n  echo     MachineGuid    REG_SZ    %CCP_FAKE_MACHINE_ID%\r\n  exit /b 0\r\n)\r\n\"%SystemRoot%\\System32\\reg.exe\" %*\r\n"
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_powershell_shim() -> &'static str {
    "@echo off\r\nsetlocal EnableDelayedExpansion\r\nset \"ARGS=%*\"\r\nif not \"!ARGS:MachineGuid=!\"==\"!ARGS!\" if not \"%CCP_FAKE_MACHINE_ID%\"==\"\" (\r\n  echo %CCP_FAKE_MACHINE_ID%\r\n  exit /b 0\r\n)\r\nif not \"!ARGS:Win32_ComputerSystemProduct=!\"==\"!ARGS!\" if not \"!ARGS:UUID=!\"==\"!ARGS!\" if not \"%CCP_FAKE_PLATFORM_UUID%\"==\"\" (\r\n  echo %CCP_FAKE_PLATFORM_UUID%\r\n  exit /b 0\r\n)\r\nif not \"!ARGS:Get-NetAdapter=!\"==\"!ARGS!\" if not \"%CCP_FAKE_MAC_ADDRESS%\"==\"\" (\r\n  set \"MAC=%CCP_FAKE_MAC_ADDRESS::=-%\"\r\n  echo !MAC!\r\n  exit /b 0\r\n)\r\nif not \"!ARGS:MacAddress=!\"==\"!ARGS!\" if not \"%CCP_FAKE_MAC_ADDRESS%\"==\"\" (\r\n  set \"MAC=%CCP_FAKE_MAC_ADDRESS::=-%\"\r\n  echo !MAC!\r\n  exit /b 0\r\n)\r\nif not \"!ARGS:COMPUTERNAME=!\"==\"!ARGS!\" if not \"%CCP_FAKE_HOSTNAME%\"==\"\" (\r\n  echo %CCP_FAKE_HOSTNAME%\r\n  exit /b 0\r\n)\r\n\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" %*\r\n"
}

fn write_text(path: &Path, contents: &str) -> Result<(), StoreError> {
    let mut file = create_secure_file(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn write_executable(path: &Path, contents: &str) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;

    write_text(path, contents)?;
    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_shim_templates_cover_hostname_machine_guid_uuid_and_mac_queries() {
        let hostname = windows_hostname_shim();
        let getmac = windows_getmac_shim();
        let wmic = windows_wmic_shim();
        let reg = windows_reg_shim();
        let powershell = windows_powershell_shim();

        assert!(hostname.contains("CCP_FAKE_HOSTNAME"));
        assert!(getmac.contains("CCP_FAKE_MAC_ADDRESS"));
        assert!(wmic.contains("CCP_FAKE_PLATFORM_UUID"));
        assert!(wmic.contains("CCP_FAKE_MAC_ADDRESS"));
        assert!(reg.contains("MachineGuid"));
        assert!(reg.contains("CCP_FAKE_MACHINE_ID"));
        assert!(powershell.contains("MachineGuid"));
        assert!(powershell.contains("Win32_ComputerSystemProduct"));
        assert!(powershell.contains("CCP_FAKE_PLATFORM_UUID"));
        assert!(powershell.contains("CCP_FAKE_MAC_ADDRESS"));
        assert!(powershell.contains("CCP_FAKE_HOSTNAME"));
    }
}
