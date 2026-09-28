//! TLS Layer
//!
//! Production TLS support using rustls for secure connections.
//! Includes session resumption and ALPN negotiation for HTTP/2.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, OnceLock};

use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use rustls::pki_types::ServerName;

use crate::tcp::TcpConnection;

/// TLS configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsConfig {
    /// Enable session resumption
    pub session_resumption: bool,
    /// ALPN protocols (e.g., ["h2", "http/1.1"])
    pub alpn_protocols: Vec<String>,
    /// Verify certificates
    pub verify_certs: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            session_resumption: true,
            alpn_protocols: vec!["h2".into(), "http/1.1".into()],
            verify_certs: true,
        }
    }
}

/// TLS connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsState {
    /// Not connected
    Disconnected,
    /// Handshaking
    Handshaking,
    /// Connected and ready
    Connected,
    /// Shutdown in progress
    Shutdown,
    /// Error occurred
    Error,
}

/// Well-known locations of the system CA bundle on Unix-like systems
#[cfg(unix)]
const SYSTEM_CA_BUNDLES: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt",                // Debian, Ubuntu, Arch, Gentoo
    "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem", // Fedora, RHEL, CentOS
    "/etc/pki/tls/certs/ca-bundle.crt",                  // older Fedora/RHEL
    "/etc/ssl/ca-bundle.pem",                            // openSUSE
    "/etc/ssl/cert.pem",                                 // Alpine, BSDs, macOS
];

/// Add the operating system's trusted roots: the file named by
/// `SSL_CERT_FILE`, plus the distribution CA bundle on Unix. This picks up
/// enterprise and user-installed CAs, as other browsers do.
fn add_system_roots(store: &mut RootCertStore) -> usize {
    use rustls::pki_types::CertificateDer;
    use rustls::pki_types::pem::PemObject;

    let mut bundles: Vec<std::path::PathBuf> = Vec::new();
    if let Some(file) = std::env::var_os("SSL_CERT_FILE") {
        bundles.push(file.into());
    }
    #[cfg(unix)]
    if let Some(bundle) = SYSTEM_CA_BUNDLES.iter().map(std::path::Path::new).find(|p| p.is_file()) {
        bundles.push(bundle.to_path_buf());
    }

    let mut added = 0;
    for bundle in bundles {
        let Ok(certs) = CertificateDer::pem_file_iter(&bundle) else { continue };
        let (valid, _invalid) = store.add_parsable_certificates(certs.filter_map(Result::ok));
        added += valid;
    }
    added
}

/// Trusted root certificates (Mozilla's set plus the system's), parsed
/// once per process
fn root_store() -> Arc<RootCertStore> {
    static ROOTS: OnceLock<Arc<RootCertStore>> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let mut root_store = RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let system = add_system_roots(&mut root_store);
        tracing::debug!("Loaded {} system root certificates", system);
        Arc::new(root_store)
    }).clone()
}

/// Get the rustls client configuration for `config`.
///
/// The default configuration is built once and shared by every connection.
/// Besides avoiding a rebuild per connection, this is what makes session
/// resumption work: the resumption cache lives inside the `ClientConfig`,
/// so a fresh config per connection could never resume a session.
fn client_config(config: &TlsConfig) -> Arc<ClientConfig> {
    static DEFAULT: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    if *config == TlsConfig::default() {
        DEFAULT.get_or_init(|| create_client_config(config)).clone()
    } else {
        create_client_config(config)
    }
}

/// Create the rustls client configuration
fn create_client_config(config: &TlsConfig) -> Arc<ClientConfig> {
    let mut tls_config = ClientConfig::builder()
        .with_root_certificates(root_store())
        .with_no_client_auth();
    
    // Configure ALPN protocols
    if !config.alpn_protocols.is_empty() {
        let alpn: Vec<Vec<u8>> = config.alpn_protocols
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();
        tls_config.alpn_protocols = alpn;
    }
    
    // Enable session resumption
    if config.session_resumption {
        tls_config.resumption = rustls::client::Resumption::default();
    }
    
    Arc::new(tls_config)
}

/// TLS stream wrapper over TCP using rustls
pub struct TlsStream {
    /// Rustls stream owning the connection
    stream: StreamOwned<ClientConnection, TcpStream>,
    /// Connection state
    state: TlsState,
    /// Negotiated ALPN protocol
    alpn: Option<String>,
    /// Server name (SNI)
    server_name: String,
}

impl TlsStream {
    /// Connect with TLS to a host using an existing TCP connection
    pub fn connect(tcp: TcpConnection, server_name: &str, config: TlsConfig) -> io::Result<Self> {
        let tls_config = client_config(&config);
        
        // Parse server name for SNI
        let server_name_parsed: ServerName<'static> = server_name
            .to_string()
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid server name"))?;
        
        // Create client connection
        let conn = ClientConnection::new(tls_config, server_name_parsed)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        
        // Take the inner TCP stream for rustls
        let tcp_stream = tcp.into_inner();
        
        // Create the StreamOwned which handles handshake automatically
        let mut stream = StreamOwned::new(conn, tcp_stream);
        
        // Perform handshake by doing a zero-byte write (forces handshake)
        // The StreamOwned will handle the handshake transparently
        stream.flush()?;
        
        // Get negotiated ALPN protocol
        let alpn = stream.conn.alpn_protocol()
            .map(|bytes| String::from_utf8_lossy(bytes).to_string());
        
        Ok(Self {
            stream,
            state: TlsState::Connected,
            alpn,
            server_name: server_name.to_string(),
        })
    }
    
    /// Get negotiated ALPN protocol
    pub fn alpn_protocol(&self) -> Option<&str> {
        self.alpn.as_deref()
    }
    
    /// Check if HTTP/2 was negotiated
    pub fn is_h2(&self) -> bool {
        self.alpn.as_deref() == Some("h2")
    }
    
    /// Get connection state
    pub fn state(&self) -> TlsState {
        self.state
    }
    
    /// Get server name
    pub fn server_name(&self) -> &str {
        &self.server_name
    }
    

    
    /// Get negotiated protocol version
    pub fn protocol_version(&self) -> Option<&'static str> {
        self.stream.conn.protocol_version().map(|v| match v {
            rustls::ProtocolVersion::TLSv1_2 => "TLSv1.2",
            rustls::ProtocolVersion::TLSv1_3 => "TLSv1.3",
            _ => "Unknown",
        })
    }
    
    /// Initiate shutdown
    pub fn shutdown(&mut self) -> io::Result<()> {
        self.state = TlsState::Shutdown;
        self.stream.conn.send_close_notify();
        self.stream.flush()
    }
}

impl Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }
    
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

/// Create a TLS connection to host:port
pub fn connect_tls(host: &str, port: u16) -> io::Result<TlsStream> {
    let addr = format!("{}:{}", host, port);
    let tcp = TcpConnection::connect(&addr)?;
    TlsStream::connect(tcp, host, TlsConfig::default())
}

/// Create a TLS connection with custom config
pub fn connect_tls_with_config(host: &str, port: u16, config: TlsConfig) -> io::Result<TlsStream> {
    let addr = format!("{}:{}", host, port);
    let tcp = TcpConnection::connect(&addr)?;
    TlsStream::connect(tcp, host, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tls_config_default() {
        let config = TlsConfig::default();
        assert!(config.session_resumption);
        assert!(config.verify_certs);
        assert!(config.alpn_protocols.contains(&"h2".to_string()));
    }
    
    #[test]
    fn test_tls_state() {
        assert_ne!(TlsState::Connected, TlsState::Handshaking);
    }
    
    #[test]
    fn test_create_client_config() {
        let config = TlsConfig::default();
        let client_config = create_client_config(&config);
        
        // Verify ALPN is configured
        assert_eq!(client_config.alpn_protocols.len(), 2);
    }

    #[test]
    fn test_default_client_config_is_shared() {
        // Sharing the config is what lets TLS sessions resume across connections
        let a = client_config(&TlsConfig::default());
        let b = client_config(&TlsConfig::default());
        assert!(Arc::ptr_eq(&a, &b));

        let custom = TlsConfig { alpn_protocols: vec!["http/1.1".into()], ..TlsConfig::default() };
        assert!(!Arc::ptr_eq(&a, &client_config(&custom)));
    }
}
