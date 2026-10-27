//! Native console launch arguments, including GNS3's custom console command.
use opsterm_core::domain::TelnetSpec;

#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    Local,
    Telnet { spec: TelnetSpec, title: String },
    Help,
    Version,
}

pub const HELP: &str = "OpsTerm terminal client

Usage:
  opsterm-egui
  opsterm-egui --telnet HOST [--port PORT] [--title NAME]
  opsterm-egui --check-update
  opsterm-egui --install-update
  opsterm-egui --help
  opsterm-egui --version

Display diagnosis: OPSTERM_DISPLAY_BACKEND=auto|x11|wayland
Third-party licenses/source: https://github.com/Setiyoaryo/ops-term/blob/main/packaging/linux/THIRD_PARTY_NOTICES.txt

Telnet defaults to port 23. Each launch opens a separate window.
GNS3 Console application command for Telnet:
  /absolute/path/opsterm-egui --telnet \"{host}\" --port {port} --title \"{name}\"
";

pub fn parse(args: &[String]) -> Result<Launch, String> {
    if args.is_empty() {
        return Ok(Launch::Local);
    }
    if args.len() == 1 {
        match args[0].as_str() {
            "--help" | "-h" => return Ok(Launch::Help),
            "--version" => return Ok(Launch::Version),
            _ => {}
        }
    }
    let mut host = None;
    let mut port = None;
    let mut title = None;
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let slot = match flag.as_str() {
            "--telnet" => &mut host,
            "--port" => &mut port,
            "--title" => &mut title,
            _ => return Err("Unknown argument; use --help for supported options".into()),
        };
        if slot.is_some() {
            return Err(format!("Duplicate {flag} argument"));
        }
        *slot = Some(
            iter.next()
                .ok_or_else(|| format!("Missing value for {flag}"))?
                .as_str(),
        );
    }
    let host = host.ok_or("--port and --title require --telnet HOST")?;
    let bracketed = host.starts_with('[');
    let host = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    let host = opsterm_core::security::validate_host(host).map_err(|e| e.to_string())?;
    if host.contains(['[', ']'])
        || (host.contains(':')
            && host
                .split('%')
                .next()
                .unwrap_or(&host)
                .parse::<std::net::Ipv6Addr>()
                .is_err())
    {
        return Err("Invalid console host or IPv6 address".into());
    }
    // Brackets only represent IPv6 literals, never a hostname.
    if bracketed && !host.contains(':') {
        return Err("Brackets require an IPv6 address".into());
    }
    let port = match port {
        None => 23,
        Some(text) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => text
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .ok_or("Port must be between 1 and 65535")?,
        Some(_) => return Err("Port must be between 1 and 65535".into()),
    };
    let address = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let title = title.unwrap_or(&address);
    if title.trim().is_empty() || title.len() > 256 || title.chars().any(char::is_control) {
        return Err("Console title must contain 1–256 bytes without control characters".into());
    }
    Ok(Launch::Telnet {
        spec: TelnetSpec {
            host,
            port,
            username: None,
            term: Some("xterm-256color".into()),
            password: None,
        },
        title: title.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_keeps_local_shell() {
        assert_eq!(parse(&[]).unwrap(), Launch::Local);
    }

    #[test]
    fn gns3_opens_named_telnet_console_without_credentials() {
        let Launch::Telnet { spec, title } = parse(&args(&[
            "--telnet",
            "127.0.0.1",
            "--port",
            "5001",
            "--title",
            "R1 lab $(literal)",
        ]))
        .unwrap() else {
            panic!("expected Telnet console")
        };
        assert_eq!(spec.host, "127.0.0.1");
        assert_eq!(spec.port, 5001);
        assert_eq!(title, "R1 lab $(literal)");
        assert_eq!(spec.username, None);
        assert_eq!(spec.password, None);
    }

    #[test]
    fn telnet_defaults_to_port_23_and_address_title() {
        let Launch::Telnet { spec, title } = parse(&args(&["--telnet", "router.example"])).unwrap()
        else {
            panic!("expected Telnet console")
        };
        assert_eq!(spec.port, 23);
        assert_eq!(title, "router.example:23");
    }

    #[test]
    fn ipv6_brackets_are_removed_for_socket_and_retained_in_default_title() {
        for host in ["::1", "[::1]"] {
            let Launch::Telnet { spec, title } =
                parse(&args(&["--port", "5002", "--telnet", host])).unwrap()
            else {
                panic!("expected Telnet console")
            };
            assert_eq!(spec.host, "::1");
            assert_eq!(title, "[::1]:5002");
        }
    }

    #[test]
    fn rejects_invalid_console_arguments() {
        for values in [
            vec!["--telnet"],
            vec!["--telnet", "-host"],
            vec!["--telnet", "host name"],
            vec!["--telnet", "host;touch"],
            vec!["--telnet", "[localhost]"],
            vec!["--telnet", "a:b:c"],
            vec!["--telnet", "host", "--port", "0"],
            vec!["--telnet", "host", "--port", "65536"],
            vec!["--telnet", "host", "--port", "+23"],
            vec!["--telnet", "host", "--port"],
            vec!["--telnet", "host", "--port", "23", "--port", "24"],
            vec!["--telnet", "host", "--telnet", "other"],
            vec!["--telnet", "host", "--title", ""],
            vec!["--telnet", "host", "--title", "R1\nR2"],
            vec!["--telnet", "host", "--title", "R1", "--title", "R2"],
            vec!["--port", "23"],
            vec!["--title", "R1"],
            vec!["--unknown"],
            vec!["--help", "--telnet", "host"],
        ] {
            assert!(parse(&args(&values)).is_err(), "accepted {values:?}");
        }
        assert!(parse(&args(&["--telnet", "host", "--title", &"R".repeat(257)])).is_err());
    }

    #[test]
    fn help_and_version_do_not_need_gui() {
        assert_eq!(parse(&args(&["--help"])).unwrap(), Launch::Help);
        assert_eq!(parse(&args(&["-h"])).unwrap(), Launch::Help);
        assert_eq!(parse(&args(&["--version"])).unwrap(), Launch::Version);
    }
}
