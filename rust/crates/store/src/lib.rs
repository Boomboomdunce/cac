pub mod blocked_hosts_store;
pub mod cert_store;
pub mod claude_config_store;
pub mod error;
pub mod identity_store;
pub mod layout;
pub mod profile_store;
pub mod runtime_state_store;
pub mod secret_store;
pub mod shim_store;

pub use blocked_hosts_store::materialize_blocked_hosts_file;
pub use cert_store::{certificate_material, ensure_profile_certificates, CertificateMaterial};
pub use claude_config_store::{
    materialize_managed_claude_config, snapshot_user_claude_provider, ManagedClaudeConfig,
};
pub use error::StoreError;
pub use identity_store::{
    ensure_profile_identity, ensure_profile_identity_seeded, identity_material,
    load_profile_identity, IdentityMaterial, ProfileIdentity,
};
pub use layout::StateLayout;
pub use profile_store::{canonical_name, ProfileStore};
pub use runtime_state_store::RuntimeStateStore;
pub use shim_store::{ensure_runtime_shims, RuntimeShimSet};
