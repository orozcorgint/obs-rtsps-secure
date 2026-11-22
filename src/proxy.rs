//! A loopback-only RTSP-to-TLS bridge. The remote leaf certificate is pinned.
//!
//! OBS connects to a private loopback listener; the camera connection is always
//! TLS, and the peer must present the explicitly configured leaf certificate.

use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme};
use url::Url;
use x509_parser::prelude::{parse_x509_certificate, ASN1Time};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const IDLE_POLL: Duration = Duration::from_millis(5);
const MAX_PENDING: usize = 2 * 1024 * 1024;
const MAX_CERT_FILE: u64 = 1024 * 1024;
const MAX_RTSP_MESSAGE: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub remote: SocketAddr,
    pub path_and_query: String,
    pub certificate: Vec<u8>,
}

impl Config {
    pub fn parse(url: &str, cert_path: &str) -> Result<Self, String> {
        let (remote, path) = parse_endpoint(url)?;
        let path_ref = Path::new(cert_path);
        if cert_path.trim().is_empty() {
            return Err("Select the camera PEM certificate".into());
        }
        let metadata =
            fs::metadata(path_ref).map_err(|e| format!("Could not read certificate: {e}"))?;
        if metadata.len() > MAX_CERT_FILE {
            return Err("The certificate file is too large".into());
        }
        let bytes = fs::read(path_ref).map_err(|e| format!("Could not read certificate: {e}"))?;
        let certs: Vec<_> = rustls_pemfile::certs(&mut BufReader::new(&bytes[..]))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("Invalid PEM certificate: {e}"))?;
        if certs.len() != 1 {
            return Err("The PEM must contain exactly one server certificate".into());
        }
        parse_x509_certificate(certs[0].as_ref()).map_err(|_| "Invalid X.509 certificate")?;
        Ok(Self {
            remote,
            path_and_query: path,
            certificate: certs[0].as_ref().to_vec(),
        })
    }
}

pub(crate) fn parse_endpoint(input: &str) -> Result<(SocketAddr, String), String> {
    let url = Url::parse(input).map_err(|e| format!("Invalid URL: {e}"))?;
    if url.scheme() != "rtsps" {
        return Err("The URL must start with rtsps://".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("This version does not support credentials in the URL".into());
    }
    if url.fragment().is_some() {
        return Err("The URL cannot contain a fragment (#)".into());
    }
    let host = url.host_str().ok_or("The camera IP address is missing")?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let ip: IpAddr = host
        .parse()
        .map_err(|_| "Use a numeric IP address in this version")?;
    let mut path = url.path().to_owned();
    if path.is_empty() {
        path.push('/');
    }
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    Ok((SocketAddr::new(ip, url.port().unwrap_or(322)), path))
}

#[derive(Debug)]
struct PinnedCertificate {
    certificate: Vec<u8>,
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedCertificate {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if end_entity.as_ref() != self.certificate.as_slice() {
            return Err(rustls::Error::General(
                "The certificate does not match the pinned certificate".into(),
            ));
        }
        let (_, cert) = parse_x509_certificate(end_entity.as_ref())
            .map_err(|_| rustls::Error::General("Invalid X.509 certificate".into()))?;
        let timestamp = i64::try_from(now.as_secs())
            .map_err(|_| rustls::Error::General("Invalid date".into()))?;
        let now = ASN1Time::from_timestamp(timestamp)
            .map_err(|_| rustls::Error::General("Invalid date".into()))?;
        if !cert.validity().is_valid_at(now) {
            return Err(rustls::Error::General(
                "The certificate is not yet valid or has expired".into(),
            ));
        }
        // The exact leaf certificate is the identity; no CA or hostname is
        // implicitly trusted. Signature verification below proves key possession.
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

pub struct Proxy {
    pub local_url: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Proxy {
    pub fn start(config: Config) -> Result<Self, String> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|e| format!("Could not open the local bridge: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let local_url = format!("rtsp://{address}{}", config.path_and_query);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("rtsps-secure-bridge".into())
            .spawn(move || {
                run_listener(listener, config, worker_stop);
            })
            .map_err(|e| format!("Could not start the bridge: {e}"))?;
        Ok(Self {
            local_url,
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_listener(listener: TcpListener, config: Config, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((client, addr)) if addr.ip().is_loopback() => {
                if let Err(error) = relay(client, &config, &stop) {
                    if !stop.load(Ordering::Acquire) {
                        crate::log(200, &format!("Bridge: {error}"));
                    }
                }
            }
            Ok((_client, _addr)) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(30))
            }
            Err(e) => {
                crate::log(200, &format!("Bridge error: {e}"));
                break;
            }
        }
    }
}

fn relay(mut local: TcpStream, config: &Config, stop: &AtomicBool) -> Result<(), String> {
    let mut guard = InboundGuard::new(local.local_addr().map_err(|e| e.to_string())?)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = PinnedCertificate {
        certificate: config.certificate.clone(),
        algorithms: provider.signature_verification_algorithms,
    };
    let tls_config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    let name = ServerName::IpAddress(config.remote.ip().into());
    let mut tls = ClientConnection::new(Arc::new(tls_config), name).map_err(|e| e.to_string())?;
    tls.set_buffer_limit(Some(MAX_PENDING));
    let mut remote = TcpStream::connect_timeout(&config.remote, CONNECT_TIMEOUT)
        .map_err(|e| format!("Connection: {e}"))?;
    remote
        .set_read_timeout(Some(CONNECT_TIMEOUT))
        .map_err(|e| e.to_string())?;
    remote
        .set_write_timeout(Some(CONNECT_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    while tls.is_handshaking() && !stop.load(Ordering::Acquire) {
        if start.elapsed() > Duration::from_secs(5) {
            return Err("TLS handshake timed out".into());
        }
        tls.complete_io(&mut remote)
            .map_err(|e| format!("TLS/certificate: {e}"))?;
    }
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    crate::log(
        300,
        "TLS connection established with the pinned certificate",
    );
    remote.set_nonblocking(true).map_err(|e| e.to_string())?;
    local.set_nonblocking(true).map_err(|e| e.to_string())?;
    let mut to_remote = Vec::<u8>::new();
    let mut to_local = Vec::<u8>::new();
    let mut remote_offset = 0;
    let mut local_offset = 0;
    let mut buf = [0u8; 32 * 1024];
    while !stop.load(Ordering::Acquire) {
        let mut progressed = false;
        if to_remote.len() - remote_offset < MAX_PENDING {
            match local.read(&mut buf) {
                Ok(0) => return Ok(()),
                Ok(n) => {
                    to_remote.extend_from_slice(&buf[..n]);
                    progressed = true;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(format!("Lectura local: {e}")),
            }
        }
        if remote_offset < to_remote.len() {
            match tls.writer().write(&to_remote[remote_offset..]) {
                Ok(n) => {
                    remote_offset += n;
                    progressed |= n > 0;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(format!("TLS encryption: {e}")),
            }
            if remote_offset == to_remote.len() {
                to_remote.clear();
                remote_offset = 0;
            } else if remote_offset > 64 * 1024 {
                to_remote.drain(..remote_offset);
                remote_offset = 0;
            }
        }
        while tls.wants_write() {
            match tls.write_tls(&mut remote) {
                Ok(0) => break,
                Ok(_) => progressed = true,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(format!("TLS write: {e}")),
            }
        }
        if to_local.len() - local_offset < MAX_PENDING {
            match tls.read_tls(&mut remote) {
                Ok(0) => return Ok(()),
                Ok(_) => {
                    tls.process_new_packets()
                        .map_err(|e| format!("TLS read: {e}"))?;
                    progressed = true;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(format!("TLS read: {e}")),
            }
            loop {
                match tls.reader().read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        guard.push(&buf[..n], &mut to_local)?;
                        progressed = true;
                        if to_local.len() - local_offset >= MAX_PENDING {
                            break;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => return Err(format!("TLS decryption: {e}")),
                }
            }
        }
        if local_offset < to_local.len() {
            match local.write(&to_local[local_offset..]) {
                Ok(0) => return Ok(()),
                Ok(n) => {
                    local_offset += n;
                    progressed = true;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(format!("Local write: {e}")),
            }
            if local_offset == to_local.len() {
                to_local.clear();
                local_offset = 0;
            } else if local_offset > 64 * 1024 {
                to_local.drain(..local_offset);
                local_offset = 0;
            }
        }
        if !progressed {
            thread::sleep(IDLE_POLL);
        }
    }
    Ok(())
}

/// Refuse server-controlled redirects and SDP control URLs that could make
/// OBS bypass the TLS tunnel. RTP/RTCP must be interleaved on this connection.
struct InboundGuard {
    data: Vec<u8>,
    base: Url,
}
impl InboundGuard {
    fn new(local: SocketAddr) -> Result<Self, String> {
        let base = Url::parse(&format!("rtsp://{local}/")).map_err(|e| e.to_string())?;
        Ok(Self {
            data: Vec::new(),
            base,
        })
    }

    fn safe_reference(&self, reference: &str) -> bool {
        self.base.join(reference.trim()).is_ok_and(|url| {
            url.scheme() == "rtsp"
                && url.host_str() == self.base.host_str()
                && url.port() == self.base.port()
                && url.username().is_empty()
                && url.password().is_none()
        })
    }

    fn validate_message(&self, message: &[u8], header_end: usize) -> Result<(), String> {
        let headers =
            std::str::from_utf8(&message[..header_end]).map_err(|_| "Invalid RTSP header")?;
        let mut lines = headers.split("\r\n");
        let status = lines.next().ok_or("Missing RTSP response")?;
        let status_code: u16 = status
            .strip_prefix("RTSP/1.0 ")
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .ok_or("Invalid RTSP response")?;
        if (300..400).contains(&status_code) {
            return Err("RTSP redirect blocked".into());
        }
        for line in lines.filter(|line| !line.is_empty()) {
            if line.starts_with([' ', '\t']) {
                return Err("Folded RTSP headers are not supported".into());
            }
            let (name, value) = line.split_once(':').ok_or("Invalid RTSP header")?;
            if name.eq_ignore_ascii_case("location") {
                return Err("RTSP redirect blocked".into());
            }
            if (name.eq_ignore_ascii_case("content-base")
                || name.eq_ignore_ascii_case("content-location"))
                && !self.safe_reference(value)
            {
                return Err("RTSP URL outside the bridge blocked".into());
            }
            if name.eq_ignore_ascii_case("transport")
                && !value.to_ascii_lowercase().contains("rtp/avp/tcp")
            {
                return Err("Non-interleaved RTSP transport blocked".into());
            }
        }
        let body =
            std::str::from_utf8(&message[header_end..]).map_err(|_| "Unsupported RTSP body")?;
        for line in body.lines() {
            let line = line.trim();
            if line
                .get(..10)
                .is_some_and(|s| s.eq_ignore_ascii_case("a=control:"))
            {
                let value = &line[10..];
                if value != "*" && !self.safe_reference(value) {
                    return Err("SDP control outside the bridge blocked".into());
                }
            }
        }
        Ok(())
    }

    fn push(&mut self, incoming: &[u8], output: &mut Vec<u8>) -> Result<(), String> {
        self.data.extend_from_slice(incoming);
        let mut used = 0;
        while used < self.data.len() {
            let pending = &self.data[used..];
            let total = if pending[0] == b'$' {
                if pending.len() < 4 {
                    break;
                }
                4 + u16::from_be_bytes([pending[2], pending[3]]) as usize
            } else {
                let Some(header_pos) = pending.windows(4).position(|w| w == b"\r\n\r\n") else {
                    if pending.len() > MAX_RTSP_MESSAGE {
                        return Err("RTSP response is too large".into());
                    }
                    break;
                };
                let header_end = header_pos + 4;
                let headers = std::str::from_utf8(&pending[..header_end])
                    .map_err(|_| "Invalid RTSP header")?;
                let mut content_length = None;
                for line in headers.split("\r\n").skip(1) {
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            let len: usize =
                                value.trim().parse().map_err(|_| "Invalid RTSP length")?;
                            if content_length.replace(len).is_some() {
                                return Err("Duplicate RTSP length".into());
                            }
                        }
                    }
                }
                let total = header_end
                    .checked_add(content_length.unwrap_or(0))
                    .ok_or("Invalid RTSP length")?;
                if total > MAX_RTSP_MESSAGE {
                    return Err("RTSP response is too large".into());
                }
                if pending.len() < total {
                    break;
                }
                self.validate_message(&pending[..total], header_end)?;
                total
            };
            if pending.len() < total {
                break;
            }
            output.extend_from_slice(&pending[..total]);
            used += total;
        }
        if used != 0 {
            self.data.drain(..used);
        }
        if self.data.len() > MAX_RTSP_MESSAGE {
            return Err("RTSP response is too large".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_insecure_url_before_reading_certificate() {
        assert!(Config::parse("rtsp://192.168.0.1/live", "")
            .unwrap_err()
            .contains("rtsps"));
    }
    #[test]
    fn rejects_credentials() {
        assert!(Config::parse("rtsps://user:pass@192.168.0.1/live", "")
            .unwrap_err()
            .contains("credentials"));
    }
    #[test]
    fn rejects_non_numeric_host() {
        assert!(Config::parse("rtsps://example.com/live", "")
            .unwrap_err()
            .contains("numeric"));
    }
    #[test]
    fn guard_forwards_fragmented_response_and_binary_media() {
        let mut guard = InboundGuard::new("127.0.0.1:1234".parse().unwrap()).unwrap();
        let mut out = vec![];
        let message = b"RTSP/1.0 200 OK\r\nContent-Base: rtsp://127.0.0.1:1234/live/\r\nContent-Length: 20\r\n\r\na=control:trackID=0\n$\x00\x00\x03abc";
        for chunk in message.chunks(3) {
            guard.push(chunk, &mut out).unwrap();
        }
        assert_eq!(out, message);
    }
    #[test]
    fn guard_rejects_redirect_and_external_sdp_control() {
        let mut guard = InboundGuard::new("127.0.0.1:1234".parse().unwrap()).unwrap();
        assert!(guard
            .push(
                b"RTSP/1.0 302 Found\r\nLocation: rtsp://example.com/\r\n\r\n",
                &mut vec![]
            )
            .is_err());
        let mut guard = InboundGuard::new("127.0.0.1:1234".parse().unwrap()).unwrap();
        let body = "a=control:rtsp://192.168.0.1/live\r\n";
        let message = format!(
            "RTSP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        assert!(guard.push(message.as_bytes(), &mut vec![]).is_err());
    }
}
