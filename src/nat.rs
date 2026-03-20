//! NAT traversal: STUN discovery + UPnP/NAT-PMP port mapping.
//!
//! Discovers the public IP and NAT type via STUN, then attempts to open
//! a port mapping using UPnP or NAT-PMP.

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

/// NAT traversal configuration.
#[derive(Debug, Clone)]
pub struct NatConfig {
    /// STUN servers to query for public IP discovery.
    pub stun_servers: Vec<String>,
    /// Attempt UPnP port mapping.
    pub enable_upnp: bool,
    /// Attempt NAT-PMP port mapping.
    pub enable_nat_pmp: bool,
    /// Preferred external port (0 = same as local).
    pub preferred_port: u16,
}

impl Default for NatConfig {
    fn default() -> Self {
        Self {
            stun_servers: vec![
                "stun.l.google.com:19302".to_string(),
                "stun1.l.google.com:19302".to_string(),
            ],
            enable_upnp: true,
            enable_nat_pmp: true,
            preferred_port: 0,
        }
    }
}

/// Result of NAT discovery and port mapping attempts.
#[derive(Debug, Clone)]
pub struct NatResult {
    /// Discovered public IP (if STUN succeeded).
    pub public_ip: Option<String>,
    /// Mapped external port (if UPnP/NAT-PMP succeeded).
    pub public_port: Option<u16>,
    /// Detected NAT type description.
    pub nat_type: String,
    /// Whether UPnP port mapping succeeded.
    pub upnp_success: bool,
    /// Whether NAT-PMP port mapping succeeded.
    pub nat_pmp_success: bool,
    /// Combined public URL if available.
    pub public_url: Option<String>,
}

/// Discover public IP via STUN and attempt port mapping.
pub async fn discover_and_map(config: &NatConfig, local_port: u16) -> NatResult {
    let mut result = NatResult {
        public_ip: None,
        public_port: None,
        nat_type: "unknown".to_string(),
        upnp_success: false,
        nat_pmp_success: false,
        public_url: None,
    };

    // Step 1: STUN discovery
    for server in &config.stun_servers {
        if let Some((ip, port)) = stun_discover(server) {
            result.public_ip = Some(ip.clone());
            result.nat_type = if port == local_port {
                "full-cone".to_string()
            } else {
                "port-restricted".to_string()
            };
            break;
        }
    }

    let ext_port = if config.preferred_port > 0 {
        config.preferred_port
    } else {
        local_port
    };

    // Step 2: UPnP port mapping
    if config.enable_upnp {
        result.upnp_success = upnp_add_mapping(local_port, ext_port).await;
        if result.upnp_success {
            result.public_port = Some(ext_port);
        }
    }

    // Step 3: NAT-PMP port mapping (fallback if UPnP failed)
    if config.enable_nat_pmp && !result.upnp_success {
        result.nat_pmp_success = nat_pmp_add_mapping(local_port, ext_port).await;
        if result.nat_pmp_success {
            result.public_port = Some(ext_port);
        }
    }

    // Build public URL if we have both IP and port
    if let (Some(ref ip), Some(port)) = (&result.public_ip, result.public_port) {
        result.public_url = Some(format!("http://{}:{}", ip, port));
    } else if let Some(ref ip) = result.public_ip {
        // STUN worked but no port mapping — might still be reachable if full-cone
        if result.nat_type == "full-cone" {
            result.public_url = Some(format!("http://{}:{}", ip, local_port));
        }
    }

    result
}

/// STUN Binding Request to discover public IP:port.
///
/// Sends a minimal STUN Binding Request (RFC 5389) and parses the
/// XOR-MAPPED-ADDRESS from the response.
fn stun_discover(server: &str) -> Option<(String, u16)> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.set_read_timeout(Some(Duration::from_secs(3))).ok()?;

    let addr: SocketAddr = server.parse().ok().or_else(|| {
        use std::net::ToSocketAddrs;
        server.to_socket_addrs().ok()?.next()
    })?;

    // STUN Binding Request: 20 bytes
    // Type: 0x0001 (Binding Request)
    // Length: 0x0000
    // Magic Cookie: 0x2112A442
    // Transaction ID: 12 random bytes
    let mut request = [0u8; 20];
    request[0] = 0x00;
    request[1] = 0x01; // Binding Request
    // Length = 0
    request[4] = 0x21;
    request[5] = 0x12;
    request[6] = 0xA4;
    request[7] = 0x42; // Magic Cookie
    // Transaction ID: use simple values
    for i in 8..20 {
        request[i] = (i as u8).wrapping_mul(7);
    }

    socket.send_to(&request, addr).ok()?;

    let mut buf = [0u8; 512];
    let (len, _) = socket.recv_from(&mut buf).ok()?;

    if len < 20 {
        return None;
    }

    // Parse response — look for XOR-MAPPED-ADDRESS (type 0x0020)
    let magic = [0x21u8, 0x12, 0xA4, 0x42];
    let mut pos = 20; // skip header
    while pos + 4 <= len {
        let attr_type = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let attr_len = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]) as usize;
        pos += 4;

        if attr_type == 0x0020 && attr_len >= 8 && pos + attr_len <= len {
            // XOR-MAPPED-ADDRESS
            let family = buf[pos + 1];
            if family == 0x01 {
                // IPv4
                let xor_port = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]);
                let port = xor_port ^ 0x2112; // XOR with magic cookie first 2 bytes

                let ip = format!(
                    "{}.{}.{}.{}",
                    buf[pos + 4] ^ magic[0],
                    buf[pos + 5] ^ magic[1],
                    buf[pos + 6] ^ magic[2],
                    buf[pos + 7] ^ magic[3],
                );

                return Some((ip, port));
            }
        }

        // Align to 4-byte boundary
        pos += (attr_len + 3) & !3;
    }

    // Fallback: look for MAPPED-ADDRESS (type 0x0001, non-XOR)
    pos = 20;
    while pos + 4 <= len {
        let attr_type = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let attr_len = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]) as usize;
        pos += 4;

        if attr_type == 0x0001 && attr_len >= 8 && pos + attr_len <= len {
            let family = buf[pos + 1];
            if family == 0x01 {
                let port = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]);
                let ip = format!(
                    "{}.{}.{}.{}",
                    buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7],
                );
                return Some((ip, port));
            }
        }

        pos += (attr_len + 3) & !3;
    }

    None
}

/// Attempt UPnP IGD port mapping.
///
/// Sends SSDP discovery on the LAN, finds the IGD, and requests a port mapping.
/// This is a best-effort implementation — silently returns false on failure.
async fn upnp_add_mapping(local_port: u16, external_port: u16) -> bool {
    // UPnP SSDP M-SEARCH for Internet Gateway Device
    let ssdp_request = format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: 239.255.255.250:1900\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: 2\r\n\
         ST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\
         \r\n"
    );

    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let _ = socket.set_read_timeout(Some(Duration::from_secs(3)));

    let ssdp_addr: SocketAddr = "239.255.255.250:1900".parse().unwrap_or_else(|_| {
        SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::new(239, 255, 255, 250)), 1900)
    });
    if socket.send_to(ssdp_request.as_bytes(), ssdp_addr).is_err() {
        return false;
    }

    let mut buf = [0u8; 2048];
    let response = match socket.recv_from(&mut buf) {
        Ok((len, _)) => String::from_utf8_lossy(&buf[..len]).to_string(),
        Err(_) => return false,
    };

    // Extract LOCATION URL from response
    let location = response
        .lines()
        .find(|l| l.to_lowercase().starts_with("location:"))
        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()));

    let location = match location {
        Some(l) => l,
        None => return false,
    };

    // Get the local IP that can reach the gateway
    let local_ip = get_local_ip().unwrap_or_else(|| "0.0.0.0".to_string());

    // Send AddPortMapping SOAP request
    let soap_body = format!(
        r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:AddPortMapping xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:1">
      <NewRemoteHost></NewRemoteHost>
      <NewExternalPort>{}</NewExternalPort>
      <NewProtocol>TCP</NewProtocol>
      <NewInternalPort>{}</NewInternalPort>
      <NewInternalClient>{}</NewInternalClient>
      <NewEnabled>1</NewEnabled>
      <NewPortMappingDescription>ai_assistant_core</NewPortMappingDescription>
      <NewLeaseDuration>3600</NewLeaseDuration>
    </u:AddPortMapping>
  </s:Body>
</s:Envelope>"#,
        external_port, local_port, local_ip
    );

    // We need to find the control URL from the device description
    // For simplicity, try common control paths
    let base_url = location.trim_end_matches('/');
    let control_paths = [
        "/ctl/IPConn",
        "/upnp/control/WANIPConn1",
        "/igdupnp/control/WANIPConn1",
        "/ctl/IPConnection",
    ];

    let client = reqwest::Client::new();
    for path in &control_paths {
        let url = format!("{}{}", base_url, path);
        let result = client
            .post(&url)
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:WANIPConnection:1#AddPortMapping\"",
            )
            .body(soap_body.clone())
            .timeout(Duration::from_secs(5))
            .send()
            .await;

        if let Ok(resp) = result {
            if resp.status().is_success() {
                return true;
            }
        }
    }

    false
}

/// Attempt NAT-PMP port mapping (RFC 6886).
///
/// Sends a mapping request to the default gateway on port 5351.
async fn nat_pmp_add_mapping(local_port: u16, external_port: u16) -> bool {
    let gateway = match get_default_gateway() {
        Some(gw) => gw,
        None => return false,
    };

    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let _ = socket.set_read_timeout(Some(Duration::from_secs(3)));

    let gw_addr: SocketAddr = format!("{}:5351", gateway).parse().unwrap_or_else(|_| {
        SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 5351)
    });

    // NAT-PMP mapping request: 12 bytes
    // Version: 0, Opcode: 2 (TCP mapping)
    // Reserved: 0x0000
    // Internal port, External port, Lifetime (3600s)
    let mut request = [0u8; 12];
    request[0] = 0; // Version
    request[1] = 2; // Opcode: Map TCP
    // Reserved: 0
    request[4..6].copy_from_slice(&local_port.to_be_bytes());
    request[6..8].copy_from_slice(&external_port.to_be_bytes());
    request[8..12].copy_from_slice(&3600u32.to_be_bytes()); // Lifetime

    if socket.send_to(&request, gw_addr).is_err() {
        return false;
    }

    let mut buf = [0u8; 16];
    match socket.recv_from(&mut buf) {
        Ok((len, _)) if len >= 16 => {
            let result_code = u16::from_be_bytes([buf[2], buf[3]]);
            result_code == 0 // 0 = success
        }
        _ => false,
    }
}

/// Get local IP address that can reach the internet.
fn get_local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

/// Get default gateway IP (best-effort, platform-dependent).
fn get_default_gateway() -> Option<String> {
    // Try to discover gateway via a UDP connect trick
    // The route to 8.8.8.8 usually goes through the default gateway
    // We can't directly get the gateway IP this way, so try common addresses
    for candidate in &["192.168.1.1", "192.168.0.1", "10.0.0.1", "172.16.0.1"] {
        let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
        if socket.connect(format!("{}:80", candidate)).is_ok() {
            return Some(candidate.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nat_config_defaults() {
        let config = NatConfig::default();
        assert!(!config.stun_servers.is_empty());
        assert!(config.enable_upnp);
        assert!(config.enable_nat_pmp);
        assert_eq!(config.preferred_port, 0);
    }

    #[test]
    fn test_local_ip_discovery() {
        // This may fail in CI without network, but should work locally
        let ip = get_local_ip();
        if let Some(ref ip) = ip {
            assert!(!ip.is_empty());
            assert_ne!(ip, "0.0.0.0");
        }
        // Don't assert Some — might not have network
    }
}
