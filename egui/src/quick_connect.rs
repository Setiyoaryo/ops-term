//! Parse the existing Quick Connect field without starting a connection.
use opsterm_core::domain::SshSpec;
use opsterm_core::security;

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
