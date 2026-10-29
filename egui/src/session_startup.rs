//! One-shot pager setup after credentials and a live device CLI prompt.

use std::time::{Duration, Instant};

pub struct Pager {
    vendor: String,
    command: String,
    deadline: Instant,
    after_output: Option<u64>,
}

impl Pager {
    pub fn new(vendor: &str, command: String) -> Self {
        Self {
            vendor: vendor.into(),
            command,
            deadline: Instant::now() + Duration::from_secs(60),
            after_output: None,
        }
    }
    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }
    pub fn after_script_write(&mut self, output_bytes: u64) {
        self.after_output = Some(output_bytes);
    }
    pub fn ready(&self, prompt: &str, output_bytes: u64) -> bool {
        self.after_output
            .is_none_or(|previous| previous != output_bytes)
            && cli_ready(&self.vendor, prompt)
    }
    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = self.command.into_bytes();
        bytes.push(b'\r');
        bytes
    }
}

fn hostname(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || "_.:/@-~*".contains(c))
}

fn huawei(prompt: &str) -> bool {
    prompt
        .strip_prefix('<')
        .and_then(|p| p.strip_suffix('>'))
        .or_else(|| prompt.strip_prefix('[').and_then(|p| p.strip_suffix(']')))
        .is_some_and(hostname)
}

fn network(prompt: &str) -> bool {
    prompt.strip_suffix(['#', '>']).is_some_and(|body| {
        !body.is_empty()
            && body
                .chars()
                .all(|c| c.is_alphanumeric() || "_.:/@-~()".contains(c))
    })
}

fn mikrotik(prompt: &str) -> bool {
    let Some(body) = prompt.strip_prefix('[').and_then(|p| p.strip_suffix('>')) else {
        return false;
    };
    let Some((name, path)) = body.split_once(']') else {
        return false;
    };
    hostname(name)
        && path
            .trim()
            .chars()
            .all(|c| c.is_alphanumeric() || " /_.-".contains(c))
}

pub fn cli_ready(vendor: &str, prompt: &str) -> bool {
    let prompt = prompt.trim();
    if prompt.contains(['\r', '\n']) {
        return false;
    }
    match vendor {
        "huawei" => huawei(prompt),
        "cisco" | "juniper" => network(prompt),
        "mikrotik" => mikrotik(prompt),
        "custom" => {
            huawei(prompt)
                || network(prompt)
                || mikrotik(prompt)
                || prompt.strip_suffix('$').is_some_and(hostname)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_only_complete_vendor_cli_prompts() {
        for prompt in [
            "<BR-APJI>",
            "[BR-APJI]",
            "[~BR-APJI]",
            "[*BR-APJI-GigabitEthernet0/0/1]",
        ] {
            assert!(cli_ready("huawei", prompt), "{prompt}");
        }
        for prompt in [
            "",
            "Username:",
            "Password:",
            "########",
            "notice #",
            "<BR-",
            "[Y/N]:",
            "New password:",
            "NOTICE: <BR-APJI>",
            "<BR-APJI> display version",
        ] {
            assert!(!cli_ready("huawei", prompt), "{prompt}");
        }
        assert!(cli_ready("cisco", "DIST-SW(config-if)#"));
        assert!(cli_ready("juniper", "user@edge>"));
        assert!(cli_ready("mikrotik", "[admin@router] >"));
        assert!(!cli_ready("cisco", "########"));
    }
}
