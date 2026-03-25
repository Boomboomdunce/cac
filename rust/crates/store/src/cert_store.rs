use crate::{error::StoreError, layout::StateLayout, secret_store::create_secure_file};
use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, RsaKeySize, PKCS_RSA_SHA256,
};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use time::{Duration, OffsetDateTime};

#[derive(Clone, Debug)]
pub struct CertificateMaterial {
    pub ca_cert: PathBuf,
    pub ca_key: PathBuf,
    pub client_cert: PathBuf,
    pub client_key: PathBuf,
}

#[derive(Clone, Debug)]
pub struct MitmCertificateMaterial {
    pub ca_cert: PathBuf,
    pub ca_key: PathBuf,
    pub node_ca_bundle: PathBuf,
}

pub fn ensure_profile_certificates(
    layout: &StateLayout,
    profile_name: &str,
) -> Result<CertificateMaterial, StoreError> {
    let material = certificate_material(layout, profile_name);

    if !material.ca_cert.is_file() || !material.ca_key.is_file() {
        generate_ca(&material)?;
    }

    if !material.client_cert.is_file() || !material.client_key.is_file() {
        generate_client_certificate(profile_name, &material)?;
    }

    Ok(material)
}

pub fn certificate_material(layout: &StateLayout, profile_name: &str) -> CertificateMaterial {
    let certs_dir = layout.certs_dir();
    CertificateMaterial {
        ca_cert: certs_dir.join("ca/ca_cert.pem"),
        ca_key: certs_dir.join("ca/ca_key.pem"),
        client_cert: certs_dir.join(profile_name).join("client_cert.pem"),
        client_key: certs_dir.join(profile_name).join("client_key.pem"),
    }
}

pub fn ensure_mitm_certificates(
    layout: &StateLayout,
) -> Result<MitmCertificateMaterial, StoreError> {
    let material = mitm_certificate_material(layout);

    if !material.ca_cert.is_file() || !material.ca_key.is_file() {
        generate_mitm_ca(&material)?;
    }

    write_node_ca_bundle(
        &material.node_ca_bundle,
        &[
            layout.certs_dir().join("ca/ca_cert.pem"),
            material.ca_cert.clone(),
        ],
    )?;

    Ok(material)
}

pub fn mitm_certificate_material(layout: &StateLayout) -> MitmCertificateMaterial {
    let certs_dir = layout.certs_dir();
    MitmCertificateMaterial {
        ca_cert: certs_dir.join("mitm/root_ca.pem"),
        ca_key: certs_dir.join("mitm/root_ca_key.pem"),
        node_ca_bundle: certs_dir.join("mitm/node_extra_ca_bundle.pem"),
    }
}

fn generate_ca(material: &CertificateMaterial) -> Result<(), StoreError> {
    let ca_dir = material
        .ca_cert
        .parent()
        .ok_or_else(|| StoreError::CertGeneration("missing CA directory".to_string()))?;
    fs::create_dir_all(ca_dir)?;

    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "ccp-privacy-ca");
    distinguished_name.push(DnType::OrganizationName, "ccp");
    distinguished_name.push(DnType::OrganizationalUnitName, "mtls");

    let mut params = CertificateParams::default();
    params.distinguished_name = distinguished_name;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    let (not_before, not_after) = certificate_validity(Duration::days(3650));
    params.not_before = not_before;
    params.not_after = not_after;
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];

    let signing_key = KeyPair::generate_rsa_for(&PKCS_RSA_SHA256, RsaKeySize::_4096)
        .map_err(|err| StoreError::CertGeneration(format!("generating CA key: {err}")))?;
    let issuer = CertifiedIssuer::self_signed(params, signing_key)
        .map_err(|err| StoreError::CertGeneration(format!("generating CA certificate: {err}")))?;

    write_secure_text(&material.ca_key, &issuer.key().serialize_pem())?;
    write_secure_text(&material.ca_cert, &issuer.pem())?;
    Ok(())
}

fn generate_mitm_ca(material: &MitmCertificateMaterial) -> Result<(), StoreError> {
    let ca_dir = material
        .ca_cert
        .parent()
        .ok_or_else(|| StoreError::CertGeneration("missing MITM CA directory".to_string()))?;
    fs::create_dir_all(ca_dir)?;

    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "ccp-capture-mitm-ca");
    distinguished_name.push(DnType::OrganizationName, "ccp");
    distinguished_name.push(DnType::OrganizationalUnitName, "mitm");

    let mut params = CertificateParams::default();
    params.distinguished_name = distinguished_name;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    let (not_before, not_after) = certificate_validity(Duration::days(3650));
    params.not_before = not_before;
    params.not_after = not_after;
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];

    let signing_key = KeyPair::generate_rsa_for(&PKCS_RSA_SHA256, RsaKeySize::_4096)
        .map_err(|err| StoreError::CertGeneration(format!("generating MITM CA key: {err}")))?;
    let issuer = CertifiedIssuer::self_signed(params, signing_key).map_err(|err| {
        StoreError::CertGeneration(format!("generating MITM CA certificate: {err}"))
    })?;

    write_secure_text(&material.ca_key, &issuer.key().serialize_pem())?;
    write_secure_text(&material.ca_cert, &issuer.pem())?;
    Ok(())
}

fn generate_client_certificate(
    profile_name: &str,
    material: &CertificateMaterial,
) -> Result<(), StoreError> {
    let client_dir = material.client_cert.parent().ok_or_else(|| {
        StoreError::CertGeneration("missing client certificate directory".to_string())
    })?;
    fs::create_dir_all(client_dir)?;

    let issuer_key_pem = fs::read_to_string(&material.ca_key)?;
    let issuer_cert_pem = fs::read_to_string(&material.ca_cert)?;
    let issuer_key = KeyPair::from_pem(&issuer_key_pem)
        .map_err(|err| StoreError::CertGeneration(format!("loading CA key: {err}")))?;
    let issuer = Issuer::from_ca_cert_pem(&issuer_cert_pem, issuer_key)
        .map_err(|err| StoreError::CertGeneration(format!("loading CA certificate: {err}")))?;

    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, format!("ccp-client-{profile_name}"));
    distinguished_name.push(DnType::OrganizationName, "ccp");
    distinguished_name.push(
        DnType::OrganizationalUnitName,
        format!("profile-{profile_name}"),
    );

    let mut params = CertificateParams::default();
    params.distinguished_name = distinguished_name;
    let (not_before, not_after) = certificate_validity(Duration::days(365));
    params.not_before = not_before;
    params.not_after = not_after;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];

    let client_key = KeyPair::generate_rsa_for(&PKCS_RSA_SHA256, RsaKeySize::_2048)
        .map_err(|err| StoreError::CertGeneration(format!("generating client key: {err}")))?;
    let client_cert = params
        .signed_by(&client_key, &issuer)
        .map_err(|err| StoreError::CertGeneration(format!("signing client certificate: {err}")))?;

    write_secure_text(&material.client_key, &client_key.serialize_pem())?;
    write_secure_text(&material.client_cert, &client_cert.pem())?;
    Ok(())
}

fn write_secure_text(path: &PathBuf, contents: &str) -> Result<(), StoreError> {
    let mut file = create_secure_file(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn write_node_ca_bundle(path: &Path, sources: &[PathBuf]) -> Result<(), StoreError> {
    let parent = path.parent().ok_or_else(|| {
        StoreError::CertGeneration("missing Node CA bundle directory".to_string())
    })?;
    fs::create_dir_all(parent)?;

    let mut bundle = String::new();
    for source in sources {
        if !source.is_file() {
            continue;
        }

        let cert = fs::read_to_string(source)?;
        if !bundle.is_empty() {
            bundle.push('\n');
        }
        bundle.push_str(cert.trim());
        bundle.push('\n');
    }

    if bundle.trim().is_empty() {
        return Err(StoreError::CertGeneration(
            "no CA sources available for Node trust bundle".to_string(),
        ));
    }

    let mut file = create_secure_file(path)?;
    file.write_all(bundle.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn certificate_validity(valid_for: Duration) -> (OffsetDateTime, OffsetDateTime) {
    let now = OffsetDateTime::now_utc();
    (now - Duration::days(1), now + valid_for)
}

#[cfg(test)]
mod tests {
    use super::{ensure_mitm_certificates, ensure_profile_certificates};
    use crate::StateLayout;

    #[test]
    fn ensure_mitm_certificates_creates_root_and_bundle() {
        let temp = tempfile::tempdir().unwrap();
        let layout = StateLayout::new(temp.path()).unwrap();

        let profile_material = ensure_profile_certificates(&layout, "work").unwrap();
        let mitm_material = ensure_mitm_certificates(&layout).unwrap();

        assert!(mitm_material.ca_cert.is_file());
        assert!(mitm_material.ca_key.is_file());
        assert!(mitm_material.node_ca_bundle.is_file());

        let mtls_ca = std::fs::read_to_string(profile_material.ca_cert).unwrap();
        let mitm_ca = std::fs::read_to_string(&mitm_material.ca_cert).unwrap();
        let node_bundle = std::fs::read_to_string(&mitm_material.node_ca_bundle).unwrap();

        assert!(node_bundle.contains(mtls_ca.trim()));
        assert!(node_bundle.contains(mitm_ca.trim()));
    }
}
