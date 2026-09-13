//! Turning what a person types into the one address the window is bound to, and
//! reading the instance's `GET /healthz` answer.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::errors::DesktopError;

/// Domain suffixes that only ever name a machine on a private network, so plain
/// http is allowed on them. `.ts.net` is Tailscale MagicDNS.
const PRIVATE_SUFFIXES: [&str; 6] = [
    ".ts.net",
    ".local",
    ".lan",
    ".internal",
    ".home.arpa",
    ".localhost",
];

/// Normalize a typed address into the origin the window binds to.
///
/// * a missing scheme becomes `https://`
/// * `http://` survives only for a host on a private network (see
///   [`is_private_host`]); anywhere else it is [`DesktopError::InvalidUrl`],
///   because an unencrypted session cookie on the open internet is not a state
///   this client will enter
/// * the path, query, and fragment are dropped: the shell binds an origin, and
///   the portal owns every route under it
/// * credentials in the address are refused outright
pub fn normalize_url(input: &str) -> Result<Url, DesktopError> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return Err(DesktopError::InvalidUrl);
    }

    // `host:8080` is a host and a port, not a scheme, so only an explicit `://`
    // counts as one.
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };

    let mut url = Url::parse(&candidate).map_err(|_| DesktopError::InvalidUrl)?;

    if !url.username().is_empty() || url.password().is_some() {
        return Err(DesktopError::InvalidUrl);
    }
    if url.host().is_none() {
        return Err(DesktopError::InvalidUrl);
    }
    match url.scheme() {
        "https" => {}
        "http" if is_private_host(&url) => {}
        _ => return Err(DesktopError::InvalidUrl),
    }

    url.set_fragment(None);
    url.set_query(None);
    url.set_path("");
    Ok(url)
}

/// Whether this address reaches the instance without encryption, so the connect
/// page can show the private-network note next to it.
pub fn is_plaintext(url: &Url) -> bool {
    url.scheme() == "http"
}

/// Whether the host names a machine on a LAN, a tailnet, or this computer.
pub fn is_private_host(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(addr)) => is_private_v4(addr),
        Some(Host::Ipv6(addr)) => is_private_v6(addr),
        Some(Host::Domain(name)) => {
            let name = name.to_ascii_lowercase();
            name == "localhost"
                || !name.contains('.')
                || PRIVATE_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
        }
        None => false,
    }
}

fn is_private_v4(addr: Ipv4Addr) -> bool {
    let [a, b, ..] = addr.octets();
    addr.is_loopback()
        || addr.is_private()
        || addr.is_link_local()
        // 100.64.0.0/10, the carrier-grade NAT range Tailscale assigns from.
        || (a == 100 && (64..128).contains(&b))
}

fn is_private_v6(addr: Ipv6Addr) -> bool {
    let first = addr.segments()[0];
    addr.is_loopback()
        // fc00::/7 unique local, fe80::/10 link local.
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
}

/// The host as the UI names it: bare when the port is the scheme's own, host
/// and port together when it is not, so "did not answer" points at the address
/// the person actually typed.
pub fn host_label(url: &Url) -> String {
    match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_string(),
        (None, _) => url.as_str().to_string(),
    }
}

/// The body of the platform's `GET /healthz` (`kernel/src/http_mcp.rs`,
/// `health_response`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub commit: String,
    #[serde(default)]
    pub checks: BTreeMap<String, String>,
}

impl HealthResponse {
    /// The instance reports itself as serving.
    pub fn is_ok(&self) -> bool {
        self.status.eq_ignore_ascii_case("ok")
    }
}

/// Read a `/healthz` body. `None` means the answer did not come from a Nexus
/// instance, which the caller turns into [`DesktopError::not_nexus`]; the
/// required fields together are the signature no other service on a private
/// network is likely to produce by accident.
pub fn parse_health(bytes: &[u8]) -> Option<HealthResponse> {
    let parsed: HealthResponse = serde_json::from_slice(bytes).ok()?;
    if parsed.status.is_empty() || parsed.version.is_empty() {
        return None;
    }
    Some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(input: &str) -> String {
        normalize_url(input).expect("should normalize").to_string()
    }

    #[test]
    fn a_bare_host_becomes_https() {
        assert_eq!(
            normalized("nexus.tail-net.ts.net"),
            "https://nexus.tail-net.ts.net/"
        );
        assert_eq!(
            normalized("  nexus.example.com  "),
            "https://nexus.example.com/"
        );
    }

    #[test]
    fn a_host_and_port_is_not_mistaken_for_a_scheme() {
        assert_eq!(
            normalized("192.168.0.183:8080"),
            "https://192.168.0.183:8080/"
        );
    }

    #[test]
    fn paths_queries_and_fragments_are_dropped() {
        assert_eq!(
            normalized("https://nexus.example.com/lists/7?q=a#top"),
            "https://nexus.example.com/"
        );
        assert_eq!(
            normalized("https://nexus.example.com:8443/login"),
            "https://nexus.example.com:8443/"
        );
    }

    #[test]
    fn http_is_allowed_on_a_private_network_only() {
        assert_eq!(
            normalized("http://192.168.0.183:8080"),
            "http://192.168.0.183:8080/"
        );
        assert_eq!(normalized("http://10.0.0.4"), "http://10.0.0.4/");
        assert_eq!(normalized("http://172.16.3.9"), "http://172.16.3.9/");
        assert_eq!(
            normalized("http://100.101.102.103"),
            "http://100.101.102.103/"
        );
        assert_eq!(
            normalized("http://localhost:8080"),
            "http://localhost:8080/"
        );
        assert_eq!(
            normalized("http://nexus.tail-net.ts.net"),
            "http://nexus.tail-net.ts.net/"
        );
        assert_eq!(normalized("http://nexus"), "http://nexus/");
        assert_eq!(
            normalized("http://[fd7a:115c:a1e0::1]"),
            "http://[fd7a:115c:a1e0::1]/"
        );
        assert_eq!(normalized("http://[::1]:8080"), "http://[::1]:8080/");

        assert_eq!(
            normalize_url("http://nexus.example.com"),
            Err(DesktopError::InvalidUrl)
        );
        assert_eq!(
            normalize_url("http://93.184.216.34"),
            Err(DesktopError::InvalidUrl)
        );
        // 172.32/12 is outside the private range; 100.128/9 is outside CGNAT.
        assert_eq!(
            normalize_url("http://172.32.0.1"),
            Err(DesktopError::InvalidUrl)
        );
        assert_eq!(
            normalize_url("http://100.128.0.1"),
            Err(DesktopError::InvalidUrl)
        );
    }

    #[test]
    fn credentials_in_the_address_are_refused() {
        assert_eq!(
            normalize_url("https://someone@nexus.example.com"),
            Err(DesktopError::InvalidUrl)
        );
        assert_eq!(
            normalize_url("https://someone:secret@nexus.example.com"),
            Err(DesktopError::InvalidUrl)
        );
    }

    #[test]
    fn other_schemes_and_nonsense_are_refused() {
        for input in [
            "",
            "   ",
            "nexus example com",
            "file:///c:/tmp",
            "javascript:alert(1)",
            "ftp://nexus.example.com",
            "https://",
            "://nexus.example.com",
        ] {
            assert_eq!(
                normalize_url(input),
                Err(DesktopError::InvalidUrl),
                "input: {input:?}"
            );
        }
    }

    #[test]
    fn plaintext_is_flagged_for_the_connect_page_note() {
        assert!(is_plaintext(
            &normalize_url("http://192.168.0.183:8080").unwrap()
        ));
        assert!(!is_plaintext(
            &normalize_url("nexus.tail-net.ts.net").unwrap()
        ));
    }

    #[test]
    fn host_label_carries_a_non_default_port() {
        assert_eq!(
            host_label(&normalize_url("nexus.example.com").unwrap()),
            "nexus.example.com"
        );
        assert_eq!(
            host_label(&normalize_url("http://192.168.0.183:8080").unwrap()),
            "192.168.0.183:8080"
        );
        assert_eq!(
            host_label(&normalize_url("https://nexus.example.com:443").unwrap()),
            "nexus.example.com"
        );
    }

    #[test]
    fn a_nexus_health_body_parses() {
        let body = br#"{"status":"ok","version":"0.1.0","commit":"31d912d","checks":{"db":"ok","event_bus":"ok","job_runtime":"ok"}}"#;
        let health = parse_health(body).expect("should parse");
        assert!(health.is_ok());
        assert_eq!(health.version, "0.1.0");
        assert_eq!(health.checks.get("db").map(String::as_str), Some("ok"));
    }

    #[test]
    fn a_health_body_without_checks_still_parses() {
        let health = parse_health(br#"{"status":"ok","version":"0.1.0","commit":"abc"}"#)
            .expect("should parse");
        assert!(health.checks.is_empty());
    }

    #[test]
    fn a_degraded_instance_parses_but_is_not_ok() {
        let health = parse_health(br#"{"status":"degraded","version":"0.1.0","commit":"abc"}"#)
            .expect("should parse");
        assert!(!health.is_ok());
    }

    #[test]
    fn anything_else_is_not_a_nexus_instance() {
        assert!(parse_health(b"<!doctype html><html></html>").is_none());
        assert!(parse_health(br#"{"ok":true}"#).is_none());
        assert!(parse_health(br#"{"status":"","version":"","commit":""}"#).is_none());
        assert!(parse_health(b"").is_none());
    }
}
