use crate::{error::StoreError, layout::StateLayout, secret_store::create_secure_file};
use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, RsaKeySize, PKCS_RSA_SHA256,
};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use time::{Duration, OffsetDateTime};

#[derive(Clone, Debug)]
pub struct CertificateMaterial {
    pub ca_cert: PathBuf,
    pub ca_key: PathBuf,
    pub client_cert: PathBuf,
    pub client_key: PathBuf,
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

fn certificate_validity(valid_for: Duration) -> (OffsetDateTime, OffsetDateTime) {
    let now = OffsetDateTime::now_utc();
    (now - Duration::days(1), now + valid_for)
}
