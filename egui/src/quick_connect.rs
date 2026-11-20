//! Parse the existing Quick Connect field without starting a connection.
use opsterm_core::domain::{SerialSpec, SshSpec};
use opsterm_core::security;

const SERIAL_BAUDS: &[u32] = &[1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400];

fn is_serial_path(value: &str) -> bool {
    (value.len() > 5 && value.starts_with("/dev/"))
        || (value.len() > 3
            && value[..3].eq_ignore_ascii_case("com")
            && value[3..].bytes().all(|byte| byte.is_ascii_digit()))
}

/// Parse `serial <device> [baud]` or a bare serial device path.
pub fn parse_serial(raw: &str) -> Result<SerialSpec, String> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    let (explicit, input) = if lower == "serial" {
        return Err("Serial device path is required".into());
    } else if lower.starts_with("serial ") || lower.starts_with("serial:") {
        (true, trimmed[7..].trim())
    } else {
        (false, trimmed)
    };
    let mut parts = input.split_whitespace();
    let device = parts.next().ok_or("Serial device path is required")?;
    if device.chars().any(char::is_whitespace) {
        return Err("Serial device path must not contain spaces".into());
    }
    if !explicit && !is_serial_path(device) {
        return Err("Not a serial device path".into());
    }
    let baud = match parts.next() {
        None => 9600,
        Some(text) => {
            let baud = text
                .parse::<u32>()
                .map_err(|_| "Serial baud rate must be a number".to_string())?;
            if !SERIAL_BAUDS.contains(&baud) {
                return Err(format!("Unsupported serial baud rate: {baud}"));
            }
            baud
        }
    };
    if parts.next().is_some() {
        return Err("Use `serial <device> [baud]`".into());
    }
    Ok(SerialSpec {
        device: device.to_string(),
        baud,
        data_bits: 8,
        parity: "none".into(),
        stop_bits: 1,
        flow_control: "none".into(),
        newline: "crlf".into(),
    })
}

pub fn parse(raw: &str) -> Result<SshSpec, String> {
    let input = raw.trim_start();
    let input = input.strip_prefix("ssh ").unwrap_or(input).trim();
    if input.chars().any(char::is_whitespace) {
        return Err("Address must not contain spaces".into());
    }
    let (username, address) = match input.split_once('@') {
        Some((user, address)) => (
            Some(security::validate_username(user).map_err(|e| e.to_string())?),
            address,
        ),
        None => (None, input),
    };
    let (host, port_text) = if let Some(rest) = address.strip_prefix('[') {
        let (host, suffix) = rest.split_once(']').ok_or("Unclosed IPv6 address")?;
        validate_ipv6(host)?;
        let port = if suffix.is_empty() {
            None
        } else {
            Some(
                suffix
                    .strip_prefix(':')
                    .ok_or("Expected :port after IPv6 address")?,
            )
        };
        (host, port)
    } else if address.matches(':').count() == 1 {
        let (host, port) = address.split_once(':').expect("one colon");
        (host, Some(port))
    } else {
        if address.contains(':') {
            validate_ipv6(address)?;
        }
        (address, None)
    };
    if host.contains(['[', ']']) {
        return Err("Invalid address brackets".into());
    }
    let host = security::validate_host(host).map_err(|e| e.to_string())?;
    let port = match port_text {
        None => 22,
        Some(text) if !text.is_empty() && text.bytes().all(|c| c.is_ascii_digit()) => {
            let port = text
                .parse::<u16>()
                .map_err(|_| "Port must be between 1 and 65535")?;
            security::validate_port(port).map_err(|e| e.to_string())?
        }
        Some(_) => return Err("Port must be between 1 and 65535".into()),
    };
    Ok(SshSpec {
        host,
        port,
        username,
        identity_file: None,
        proxy_jump: None,
        name: None,
        term: Some("xterm-256color".into()),
        connect_timeout: Some(20),
        server_alive_interval: Some(60),
        x11_forwarding: false,
        forward_agent: false,
        compression: false,
        certificate_file: None,
        pkcs11_provider: None,
        engine: Some("openssh".into()),
        password: None,
    })
}

fn validate_ipv6(host: &str) -> Result<(), String> {
    host.split('%')
        .next()
        .unwrap_or(host)
        .parse::<std::net::Ipv6Addr>()
        .map(|_| ())
        .map_err(|_| "Invalid IPv6 address".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_username_alias_and_port() {
        let spec = parse(" ssh operator@router.example:2222 ").unwrap();
        assert_eq!(spec.host, "router.example");
        assert_eq!(spec.port, 2222);
        assert_eq!(spec.username.as_deref(), Some("operator"));
    }

    #[test]
    fn preserves_bare_ipv6_instead_of_treating_last_group_as_port() {
        let spec = parse("root@2001:db8::1234").unwrap();
        assert_eq!(spec.host, "2001:db8::1234");
        assert_eq!(spec.port, 22);
    }

    #[test]
    fn removes_ipv6_brackets_for_transport() {
        let spec = parse("root@[::1]:2222").unwrap();
        assert_eq!(spec.host, "::1");
        assert_eq!(spec.port, 2222);
        assert_eq!(parse("[fe80::1%eth0]").unwrap().host, "fe80::1%eth0");
    }

    #[test]
    fn parses_serial_paths_and_baud_rates() {
        let spec = parse_serial("/dev/ttyUSB0 115200").unwrap();
        assert_eq!(spec.device, "/dev/ttyUSB0");
        assert_eq!(spec.baud, 115200);
        assert_eq!(spec.data_bits, 8);
        assert_eq!(spec.parity, "none");
        assert_eq!(spec.stop_bits, 1);
        assert_eq!(spec.flow_control, "none");
        let spec = parse_serial("serial COM3").unwrap();
        assert_eq!(spec.device, "COM3");
        assert_eq!(spec.baud, 9600);
        assert_eq!(parse_serial("serial:/dev/ttyACM0 9600").unwrap().baud, 9600);
        assert_eq!(parse_serial("com12 19200").unwrap().baud, 19200);
        for value in [
            "router.example",
            "root@host",
            "/dev/",
            "serial",
            "serial /dev/ttyUSB0 12345",
            "serial /dev/ttyUSB0 9600 8N1",
            "serial /dev/ttyUSB0 fast",
        ] {
            assert!(parse_serial(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn rejects_malformed_addresses_and_options() {
        for value in [
            "",
            "ssh ",
            "-oProxyCommand=bad",
            "user@@host",
            "-root@host",
            "host:0",
            "host:65536",
            "host:abc",
            "host:",
            "host:+22",
            "[::1",
            "[::1]tail",
            "[example.com]",
            "a:b:c",
            "host name",
            "user@",
        ] {
            assert!(parse(value).is_err(), "accepted {value:?}");
        }
    }
}
