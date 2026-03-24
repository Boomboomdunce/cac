use crate::{error::StoreError, layout::StateLayout, secret_store::create_secure_file};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct IdentityMaterial {
    pub root: PathBuf,
    pub uuid: PathBuf,
    pub stable_id: PathBuf,
    pub user_id: PathBuf,
    pub machine_id: PathBuf,
    pub hostname: PathBuf,
    pub mac_address: PathBuf,
    pub tz: PathBuf,
    pub lang: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ProfileIdentity {
    pub uuid: String,
    pub stable_id: String,
    pub user_id: String,
    pub machine_id: String,
    pub hostname: String,
    pub mac_address: String,
    pub tz: String,
    pub lang: String,
}

pub fn ensure_profile_identity(
    layout: &StateLayout,
    profile_name: &str,
) -> Result<IdentityMaterial, StoreError> {
    ensure_profile_identity_seeded(layout, profile_name, None, None)
}

pub fn ensure_profile_identity_seeded(
    layout: &StateLayout,
    profile_name: &str,
    tz: Option<&str>,
    lang: Option<&str>,
) -> Result<IdentityMaterial, StoreError> {
    let material = identity_material(layout, profile_name);
    fs::create_dir_all(&material.root)?;

    ensure_text_file(&material.uuid, || Uuid::new_v4().to_string().to_uppercase())?;
    ensure_text_file(&material.stable_id, || {
        Uuid::new_v4().to_string().to_lowercase()
    })?;
    ensure_text_file(&material.user_id, random_user_id)?;
    ensure_text_file(&material.machine_id, || Uuid::new_v4().simple().to_string())?;
    ensure_text_file(&material.hostname, random_hostname)?;
    ensure_text_file(&material.mac_address, random_mac_address)?;
    ensure_text_file(&material.tz, || {
        tz.unwrap_or("America/New_York").to_string()
    })?;
    ensure_text_file(&material.lang, || lang.unwrap_or("en_US.UTF-8").to_string())?;

    Ok(material)
}

pub fn identity_material(layout: &StateLayout, profile_name: &str) -> IdentityMaterial {
    let root = layout.identities_dir().join(profile_name);
    IdentityMaterial {
        root: root.clone(),
        uuid: root.join("uuid"),
        stable_id: root.join("stable_id"),
        user_id: root.join("user_id"),
        machine_id: root.join("machine_id"),
        hostname: root.join("hostname"),
        mac_address: root.join("mac_address"),
        tz: root.join("tz"),
        lang: root.join("lang"),
    }
}

pub fn load_profile_identity(
    layout: &StateLayout,
    profile_name: &str,
) -> Result<ProfileIdentity, StoreError> {
    let material = ensure_profile_identity(layout, profile_name)?;
    Ok(ProfileIdentity {
        uuid: read_identity_value(&material.uuid)?,
        stable_id: read_identity_value(&material.stable_id)?,
        user_id: read_identity_value(&material.user_id)?,
        machine_id: read_identity_value(&material.machine_id)?,
        hostname: read_identity_value(&material.hostname)?,
        mac_address: read_identity_value(&material.mac_address)?,
        tz: read_identity_value(&material.tz)?,
        lang: read_identity_value(&material.lang)?,
    })
}

fn ensure_text_file(path: &PathBuf, generator: impl FnOnce() -> String) -> Result<(), StoreError> {
    if path.is_file() {
        return Ok(());
    }

    let mut file = create_secure_file(path)?;
    file.write_all(generator().as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn random_user_id() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn random_hostname() -> String {
    format!("host-{}", &Uuid::new_v4().simple().to_string()[..8])
}

fn random_mac_address() -> String {
    let token = Uuid::new_v4().simple().to_string();
    let bytes = [
        "02".to_string(),
        token[0..2].to_string(),
        token[2..4].to_string(),
        token[4..6].to_string(),
        token[6..8].to_string(),
        token[8..10].to_string(),
    ];
    bytes.join(":")
}

fn read_identity_value(path: &PathBuf) -> Result<String, StoreError> {
    let contents = fs::read_to_string(path)?;
    Ok(contents.trim().to_string())
}
