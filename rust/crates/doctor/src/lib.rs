mod checks;
pub mod report;

pub use report::{CheckResult, CheckStatus, DoctorReport};

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct DoctorConfig {
    state_root: PathBuf,
    profile: String,
}

impl DoctorConfig {
    pub fn new(state_root: PathBuf, profile: impl Into<String>) -> Self {
        DoctorConfig {
            state_root,
            profile: profile.into(),
        }
    }

    pub fn state_root(&self) -> &std::path::Path {
        &self.state_root
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }
}

pub fn run(config: DoctorConfig) -> DoctorReport {
    let mut report = DoctorReport::new();

    report.add_check(checks::profile_existence(&config));
    report.add_check(checks::state_root_layout(&config));
    report.add_check(checks::adapter_resolution(&config));
    report.add_check(checks::platform_capability_support(&config));
    report.add_check(checks::identity_materials(&config));
    report.add_check(checks::mtls_materials(&config));
    report.add_check(checks::mitm_materials(&config));
    report.add_check(checks::mitm_system_trust(&config));
    report.add_check(checks::dns_blocking(&config));
    report.add_check(checks::proxy_reachability(&config));
    report.add_check(checks::proxy_exit_ip(&config));
    report.add_check(checks::local_proxy_conflicts(&config));
    report.add_check(checks::runtime_self_audit(&config));
    report.add_check(checks::secret_permission_sanity(&config));

    report
}
