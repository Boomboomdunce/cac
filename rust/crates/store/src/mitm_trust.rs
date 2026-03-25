use crate::{cert_store::ensure_mitm_certificates, error::StoreError, layout::StateLayout};
use std::path::{Path, PathBuf};

pub const MITM_CA_COMMON_NAME: &str = "ccp-capture-mitm-ca";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MitmSystemTrustStatus {
    pub supported: bool,
    pub installed: bool,
    pub keychain: Option<PathBuf>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SecurityCommandOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

trait SecurityExecutor {
    fn run(&self, args: &[String]) -> Result<SecurityCommandOutput, StoreError>;
}

pub fn mitm_system_trust_status(layout: &StateLayout) -> Result<MitmSystemTrustStatus, StoreError> {
    mitm_system_trust_status_for_root(layout.root())
}

pub fn mitm_system_trust_status_for_root(root: &Path) -> Result<MitmSystemTrustStatus, StoreError> {
    #[cfg(target_os = "macos")]
    {
        mitm_system_trust_status_with_executor(root, &RealSecurityExecutor)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = root;
        Ok(MitmSystemTrustStatus {
            supported: false,
            installed: false,
            keychain: None,
            message: "system trust installation is not supported on this platform".to_string(),
        })
    }
}

pub fn install_mitm_system_trust(
    layout: &StateLayout,
) -> Result<MitmSystemTrustStatus, StoreError> {
    #[cfg(target_os = "macos")]
    {
        install_mitm_system_trust_with_executor(layout, &RealSecurityExecutor)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = layout;
        Err(StoreError::TrustManagement(
            "system trust installation is not supported on this platform".to_string(),
        ))
    }
}

pub fn remove_mitm_system_trust(layout: &StateLayout) -> Result<MitmSystemTrustStatus, StoreError> {
    #[cfg(target_os = "macos")]
    {
        remove_mitm_system_trust_with_executor(layout, &RealSecurityExecutor)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = layout;
        Err(StoreError::TrustManagement(
            "system trust removal is not supported on this platform".to_string(),
        ))
    }
}

#[cfg(target_os = "macos")]
fn mitm_system_trust_status_with_executor(
    root: &Path,
    executor: &dyn SecurityExecutor,
) -> Result<MitmSystemTrustStatus, StoreError> {
    let cert_path = root.join("certs/mitm/root_ca.pem");
    let keychain = login_keychain_path().ok_or_else(|| {
        StoreError::TrustManagement("could not determine login keychain path".to_string())
    })?;

    if !cert_path.is_file() {
        return Ok(MitmSystemTrustStatus {
            supported: true,
            installed: false,
            keychain: Some(keychain),
            message: "MITM CA has not been prepared yet".to_string(),
        });
    }

    let cert_pem = std::fs::read_to_string(&cert_path)?;
    let output = executor.run(&security_find_args(&keychain))?;
    if !output.success {
        return Err(StoreError::TrustManagement(format!(
            "security find-certificate failed: {}",
            output.stderr.trim()
        )));
    }

    let installed = security_output_contains_pem(&output.stdout, &cert_pem);
    Ok(MitmSystemTrustStatus {
        supported: true,
        installed,
        keychain: Some(keychain),
        message: if installed {
            "MITM root is trusted in the macOS login keychain".to_string()
        } else {
            "MITM root is not trusted in the macOS login keychain".to_string()
        },
    })
}

#[cfg(target_os = "macos")]
fn install_mitm_system_trust_with_executor(
    layout: &StateLayout,
    executor: &dyn SecurityExecutor,
) -> Result<MitmSystemTrustStatus, StoreError> {
    let material = ensure_mitm_certificates(layout)?;
    let keychain = login_keychain_path().ok_or_else(|| {
        StoreError::TrustManagement("could not determine login keychain path".to_string())
    })?;
    let before = mitm_system_trust_status_with_executor(layout.root(), executor)?;
    if before.installed {
        return Ok(before);
    }

    let output = executor.run(&security_add_args(&keychain, &material.ca_cert))?;
    if !output.success {
        return Err(StoreError::TrustManagement(format!(
            "security add-trusted-cert failed: {}",
            output.stderr.trim()
        )));
    }

    mitm_system_trust_status_with_executor(layout.root(), executor)
}

#[cfg(target_os = "macos")]
fn remove_mitm_system_trust_with_executor(
    layout: &StateLayout,
    executor: &dyn SecurityExecutor,
) -> Result<MitmSystemTrustStatus, StoreError> {
    let keychain = login_keychain_path().ok_or_else(|| {
        StoreError::TrustManagement("could not determine login keychain path".to_string())
    })?;
    let status = mitm_system_trust_status_with_executor(layout.root(), executor)?;
    if !status.installed {
        return Ok(status);
    }

    let output = executor.run(&security_delete_args(&keychain))?;
    if !output.success && !output.stderr.contains("could not find") {
        return Err(StoreError::TrustManagement(format!(
            "security delete-certificate failed: {}",
            output.stderr.trim()
        )));
    }

    mitm_system_trust_status_with_executor(layout.root(), executor)
}

#[cfg(target_os = "macos")]
fn login_keychain_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join("Library/Keychains/login.keychain-db"))
}

#[cfg(target_os = "macos")]
fn security_find_args(keychain: &Path) -> Vec<String> {
    vec![
        "find-certificate".to_string(),
        "-c".to_string(),
        MITM_CA_COMMON_NAME.to_string(),
        "-a".to_string(),
        "-p".to_string(),
        keychain.display().to_string(),
    ]
}

#[cfg(target_os = "macos")]
fn security_add_args(keychain: &Path, cert_path: &Path) -> Vec<String> {
    vec![
        "add-trusted-cert".to_string(),
        "-d".to_string(),
        "-r".to_string(),
        "trustRoot".to_string(),
        "-k".to_string(),
        keychain.display().to_string(),
        cert_path.display().to_string(),
    ]
}

#[cfg(target_os = "macos")]
fn security_delete_args(keychain: &Path) -> Vec<String> {
    vec![
        "delete-certificate".to_string(),
        "-c".to_string(),
        MITM_CA_COMMON_NAME.to_string(),
        keychain.display().to_string(),
    ]
}

#[cfg(target_os = "macos")]
fn security_output_contains_pem(output: &str, cert_pem: &str) -> bool {
    let expected = cert_pem.lines().map(str::trim).collect::<String>();
    output
        .split("-----BEGIN CERTIFICATE-----")
        .filter(|block| !block.trim().is_empty())
        .map(|block| format!("-----BEGIN CERTIFICATE-----{block}"))
        .map(|pem| pem.lines().map(str::trim).collect::<String>())
        .any(|pem| pem == expected)
}

#[cfg(target_os = "macos")]
struct RealSecurityExecutor;

#[cfg(target_os = "macos")]
impl SecurityExecutor for RealSecurityExecutor {
    fn run(&self, args: &[String]) -> Result<SecurityCommandOutput, StoreError> {
        let security_bin =
            std::env::var("CCP_SECURITY_BIN").unwrap_or_else(|_| "security".to_string());
        let output = std::process::Command::new(security_bin)
            .args(args)
            .output()
            .map_err(StoreError::Io)?;

        Ok(SecurityCommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::StateLayout;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct FakeSecurityExecutor {
        outputs: RefCell<VecDeque<SecurityCommandOutput>>,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl FakeSecurityExecutor {
        fn new(outputs: Vec<SecurityCommandOutput>) -> Self {
            Self {
                outputs: RefCell::new(VecDeque::from(outputs)),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl SecurityExecutor for FakeSecurityExecutor {
        fn run(&self, args: &[String]) -> Result<SecurityCommandOutput, StoreError> {
            self.calls.borrow_mut().push(args.to_vec());
            self.outputs.borrow_mut().pop_front().ok_or_else(|| {
                StoreError::TrustManagement("missing fake security output".to_string())
            })
        }
    }

    #[test]
    fn trust_status_detects_matching_pem_in_login_keychain() {
        let temp = tempfile::tempdir().unwrap();
        let layout = StateLayout::new(temp.path()).unwrap();
        let material = ensure_mitm_certificates(&layout).unwrap();
        let cert_pem = std::fs::read_to_string(material.ca_cert).unwrap();
        let executor = FakeSecurityExecutor::new(vec![SecurityCommandOutput {
            success: true,
            stdout: cert_pem.clone(),
            stderr: String::new(),
        }]);

        let status = mitm_system_trust_status_with_executor(layout.root(), &executor).unwrap();

        assert!(status.installed);
        assert!(executor.calls.borrow()[0].contains(&"find-certificate".to_string()));
    }

    #[test]
    fn install_trust_calls_add_trusted_cert_then_rechecks_status() {
        let temp = tempfile::tempdir().unwrap();
        let layout = StateLayout::new(temp.path()).unwrap();
        let material = ensure_mitm_certificates(&layout).unwrap();
        let cert_pem = std::fs::read_to_string(material.ca_cert).unwrap();
        let executor = FakeSecurityExecutor::new(vec![
            SecurityCommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
            SecurityCommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
            SecurityCommandOutput {
                success: true,
                stdout: cert_pem,
                stderr: String::new(),
            },
        ]);

        let status = install_mitm_system_trust_with_executor(&layout, &executor).unwrap();

        assert!(status.installed);
        let calls = executor.calls.borrow();
        assert!(calls[1].contains(&"add-trusted-cert".to_string()));
        assert_eq!(calls.len(), 3);
    }

    #[test]
    fn remove_trust_calls_delete_certificate_when_installed() {
        let temp = tempfile::tempdir().unwrap();
        let layout = StateLayout::new(temp.path()).unwrap();
        let material = ensure_mitm_certificates(&layout).unwrap();
        let cert_pem = std::fs::read_to_string(material.ca_cert).unwrap();
        let executor = FakeSecurityExecutor::new(vec![
            SecurityCommandOutput {
                success: true,
                stdout: cert_pem,
                stderr: String::new(),
            },
            SecurityCommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
            SecurityCommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
        ]);

        let status = remove_mitm_system_trust_with_executor(&layout, &executor).unwrap();

        assert!(!status.installed);
        let calls = executor.calls.borrow();
        assert!(calls[1].contains(&"delete-certificate".to_string()));
        assert_eq!(calls.len(), 3);
    }
}
