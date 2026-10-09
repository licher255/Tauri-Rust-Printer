use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};
use tiny_http::SslConfig;

#[derive(Serialize, Deserialize)]
struct Identity {
    hostname: String,
    certificate: String,
    private_key: String,
}

// Persist one certificate/key pair atomically in the user's app configuration directory.
pub fn identity(directory: &Path, hostname: &str) -> Result<SslConfig, String> {
    let path = directory.join("ipps-identity.json");
    let identity = match std::fs::read(&path) {
        Ok(data) => {
            let identity: Identity = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
            if identity.hostname != hostname {
                return Err("IPPS certificate hostname does not match this computer".into());
            }
            identity
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let rcgen::CertifiedKey { cert, key_pair } =
                rcgen::generate_simple_self_signed(vec![hostname.trim_end_matches('.').into()])
                    .map_err(|e| e.to_string())?;
            let identity = Identity {
                hostname: hostname.into(),
                certificate: cert.pem(),
                private_key: key_pair.serialize_pem(),
            };
            std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
            let mut temporary =
                tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
            temporary
                .write_all(&serde_json::to_vec(&identity).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            temporary.as_file().sync_all().map_err(|e| e.to_string())?;
            temporary
                .persist_noclobber(path)
                .map_err(|e| e.error.to_string())?;
            identity
        }
        Err(error) => return Err(error.to_string()),
    };
    Ok(SslConfig {
        certificate: identity.certificate.into_bytes(),
        private_key: identity.private_key.into_bytes(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn identity_survives_restart_and_rejects_wrong_host() {
        let dir = tempfile::tempdir().unwrap();
        let first = super::identity(dir.path(), "printer.local.").unwrap();
        let second = super::identity(dir.path(), "printer.local.").unwrap();
        assert_eq!(first.certificate, second.certificate);
        assert_eq!(first.private_key, second.private_key);
        assert!(super::identity(dir.path(), "other.local.").is_err());
    }
}
