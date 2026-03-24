use crate::{error::StoreError, layout::StateLayout, secret_store::create_secure_file};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;

pub fn materialize_blocked_hosts_file(
    layout: &StateLayout,
    blocked_hosts: &BTreeSet<String>,
) -> Result<PathBuf, StoreError> {
    let path = layout.config_dir().join("blocked_hosts");
    let mut file = create_secure_file(&path)?;
    writeln!(file, "# ccp blocked hosts")?;
    for host in blocked_hosts {
        writeln!(file, "{host}\tlocalhost")?;
    }
    file.sync_all()?;
    Ok(path)
}
