//! Explicit, untrusted certificate discovery. No RTSP request or credentials
//! are sent during discovery, and the result is never trusted automatically.
use crate::{obs, proxy};
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::digest::{digest, SHA256};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme};
use std::fs::{self, OpenOptions};
use std::io::{BufReader, ErrorKind, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use x509_parser::prelude::{parse_x509_certificate, ASN1Time};

const TIMEOUT: Duration = Duration::from_secs(2);
const CERT_MAX: usize = 1024 * 1024;

#[derive(Clone)]
enum Status {
    Idle,
    Fetching(SocketAddr),
    Ready {
        remote: SocketAddr,
        der: Vec<u8>,
        fingerprint: String,
    },
    Approved(String),
    Error(String),
}

pub struct Discovery {
    state: Arc<Mutex<Status>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Discovery {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(Status::Idle)),
            stopped: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }

    pub fn start(&mut self, url: &str, parent: *mut obs::Obs) -> Result<(), String> {
        let (remote, _) = proxy::parse_endpoint(url)?;
        if let Some(worker) = self.worker.take() {
            if !worker.is_finished() {
                self.worker = Some(worker);
                return Err("A certificate fetch is already in progress".into());
            }
            let _ = worker.join();
        }
        *self
            .state
            .lock()
            .map_err(|_| "Certificate status unavailable")? = Status::Fetching(remote);
        let state = Arc::clone(&self.state);
        let stopped = Arc::clone(&self.stopped);
        let parent_address = parent as usize;
        let worker = thread::Builder::new()
            .name("rtsps-certificate-discovery".into())
            .spawn(move || {
                let result = fetch(remote, &stopped);
                if stopped.load(Ordering::Acquire) {
                    return;
                }
                if let Ok(mut status) = state.lock() {
                    *status = match result {
                        Ok(der) => Status::Ready {
                            remote,
                            fingerprint: fingerprint(&der),
                            der,
                        },
                        Err(error) => Status::Error(error),
                    };
                }
                // The Source destructor sets stopped and joins this thread before
                // returning, so the OBS source remains valid throughout this call.
                if !stopped.load(Ordering::Acquire) {
                    unsafe { obs::obs_source_update_properties(parent_address as *mut obs::Obs) };
                }
            })
            .map_err(|e| format!("Could not start certificate discovery: {e}"))?;
        self.worker = Some(worker);
        Ok(())
    }

    pub fn message(&self) -> String {
        let Ok(status) = self.state.lock() else {
            return "Certificate status unavailable".into();
        };
        match &*status {
            Status::Idle => "Enter the device URL and select 'Fetch device certificate'. Verify its fingerprint before trusting it on first use.".into(),
            Status::Fetching(remote) => format!("Fetching the certificate from {remote}... It is not trusted yet."),
            Status::Ready { remote, fingerprint, .. } => format!("Certificate received from {remote}. SHA-256 fingerprint:\n{fingerprint}\nCompare it with the fingerprint or certificate obtained from the device through another channel. Only if it matches, select 'I verified it: use certificate'."),
            Status::Approved(fingerprint) => format!("Certificate saved. SHA-256 fingerprint: {fingerprint}. Review the selected certificate field."),
            Status::Error(error) => format!("Certificate fetch failed: {error}. No new certificate was trusted."),
        }
    }

    pub fn can_approve(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|status| matches!(*status, Status::Ready { .. }))
    }

    pub fn error(&self, error: String) {
        if let Ok(mut status) = self.state.lock() {
            *status = Status::Error(error);
        }
    }

    pub fn approve(&self, url: &str, directory: &Path) -> Result<PathBuf, String> {
        let (current_remote, _) = proxy::parse_endpoint(url)?;
        let (remote, der, fingerprint) = {
            let status = self
                .state
                .lock()
                .map_err(|_| "Certificate status unavailable")?;
            match &*status {
                Status::Ready {
                    remote,
                    der,
                    fingerprint,
                } => (*remote, der.clone(), fingerprint.clone()),
                _ => return Err("Fetch and verify the certificate first".into()),
            }
        };
        if remote != current_remote {
            return Err("The IP address or port changed; fetch the certificate again".into());
        }
        let path = save_certificate(directory, &der)?;
        if let Ok(mut status) = self.state.lock() {
            *status = Status::Approved(fingerprint);
        }
        Ok(path)
    }

    pub fn stop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug)]
struct CollectCertificate {
    captured: Mutex<Option<Vec<u8>>>,
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for CollectCertificate {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if cert.len() > CERT_MAX {
            return Err(rustls::Error::General("Certificate is too large".into()));
        }
        let (_, parsed) = parse_x509_certificate(cert.as_ref())
            .map_err(|_| rustls::Error::General("Invalid X.509 certificate".into()))?;
        let seconds = i64::try_from(now.as_secs())
            .map_err(|_| rustls::Error::General("Invalid date".into()))?;
        let now = ASN1Time::from_timestamp(seconds)
            .map_err(|_| rustls::Error::General("Invalid date".into()))?;
        if !parsed.validity().is_valid_at(now) {
            return Err(rustls::Error::General(
                "The certificate is not valid at the current date".into(),
            ));
        }
        *self
            .captured
            .lock()
            .map_err(|_| rustls::Error::General("Could not read certificate".into()))? =
            Some(cert.as_ref().to_vec());
        // Discovery ONLY: this is not trust. The handshake signatures below
        // still verify possession of the private key; no application data is sent.
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, signature, &self.algorithms)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, signature, &self.algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

fn fetch(remote: SocketAddr, stopped: &AtomicBool) -> Result<Vec<u8>, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(CollectCertificate {
        captured: Mutex::new(None),
        algorithms: provider.signature_verification_algorithms,
    });
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    let mut connection =
        ClientConnection::new(Arc::new(config), ServerName::IpAddress(remote.ip().into()))
            .map_err(|e| e.to_string())?;
    let mut tcp =
        TcpStream::connect_timeout(&remote, TIMEOUT).map_err(|e| format!("Connection: {e}"))?;
    tcp.set_read_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    tcp.set_write_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    while connection.is_handshaking() {
        if stopped.load(Ordering::Acquire) {
            return Err("Operation cancelled".into());
        }
        if start.elapsed() > Duration::from_secs(5) {
            return Err("TLS handshake timed out".into());
        }
        connection
            .complete_io(&mut tcp)
            .map_err(|e| format!("TLS: {e}"))?;
    }
    let result = verifier
        .captured
        .lock()
        .map_err(|_| "Could not read certificate".to_string())?
        .take()
        .ok_or_else(|| "The device did not send a certificate".into());
    result
}

fn fingerprint(der: &[u8]) -> String {
    digest(&SHA256, der)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn save_certificate(directory: &Path, der: &[u8]) -> Result<PathBuf, String> {
    // Portable OBS may supply a path relative to its binary directory. Save
    // an absolute path so that the scene remains usable from any working dir.
    let directory = std::path::absolute(directory)
        .map_err(|e| format!("Could not resolve the certificate folder: {e}"))?;
    fs::create_dir_all(&directory)
        .map_err(|e| format!("Could not create the certificate folder: {e}"))?;
    let filename = format!("{}.pem", fingerprint(der).replace(':', ""));
    let path = directory.join(filename);
    let encoded = STANDARD.encode(der);
    let mut pem = String::from("-----BEGIN CERTIFICATE-----\n");
    for line in encoded.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(line).map_err(|_| "Invalid PEM encoding")?);
        pem.push('\n');
    }
    pem.push_str("-----END CERTIFICATE-----\n");
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => file
            .write_all(pem.as_bytes())
            .map_err(|e| format!("Could not save the certificate: {e}"))?,
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
            if metadata.len() > CERT_MAX as u64 {
                return Err("The saved certificate is too large".into());
            }
            let existing = fs::read(&path).map_err(|e| e.to_string())?;
            let certificates = rustls_pemfile::certs(&mut BufReader::new(&existing[..]))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            if certificates.len() != 1 || certificates[0].as_ref() != der {
                return Err(
                    "A different file with that name already exists; it was not overwritten".into(),
                );
            }
        }
        Err(e) => return Err(format!("Could not save the certificate: {e}")),
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_known_value() {
        assert_eq!(fingerprint(b"abc"), "BA:78:16:BF:8F:01:CF:EA:41:41:40:DE:5D:AE:22:23:B0:03:61:A3:96:17:7A:9C:B4:10:FF:61:F2:00:15:AD");
    }
    #[test]
    fn cannot_approve_without_discovery() {
        let discovery = Discovery::new();
        assert!(discovery
            .approve("rtsps://127.0.0.1/live", Path::new("unused"))
            .is_err());
    }
    #[test]
    fn cannot_approve_a_different_address() {
        let discovery = Discovery::new();
        *discovery.state.lock().unwrap() = Status::Ready {
            remote: "127.0.0.1:8554".parse().unwrap(),
            der: vec![1, 2, 3],
            fingerprint: "fake".into(),
        };
        assert!(discovery
            .approve("rtsps://127.0.0.2:8554/live", Path::new("unused"))
            .is_err());
    }
}
