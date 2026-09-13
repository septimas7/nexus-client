//! Where the one window is allowed to go.
//!
//! FR-002 forbids a route table: the shell keeps no allowlist, denylist, or
//! page manifest for the instance origin, because that is what makes
//! reimplementing a portal screen structurally impossible rather than policed.
//! The whole policy is therefore an origin comparison, and this module is the
//! only place it is written.

use url::Url;

/// The three answers the navigation handler can give.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigation {
    /// Let the web view navigate.
    Allow,
    /// Cancel the navigation and hand the address to the system browser.
    OpenExternally,
    /// Cancel the navigation and do nothing else.
    Block,
}

/// Decide what to do with a navigation the web view is about to make.
///
/// * anything on the instance origin, at any depth, including the portal's own
///   sign-in redirect: [`Navigation::Allow`]
/// * the shell's own bundled pages: [`Navigation::Allow`]
/// * any other http or https origin: [`Navigation::OpenExternally`], so a link
///   out of the portal lands in the browser the person already uses and never
///   in a window that holds a session
/// * every other scheme: [`Navigation::Block`]
pub fn policy(instance_origin: Option<&Url>, requested_url: &Url) -> Navigation {
    if is_shell_asset(requested_url) {
        return Navigation::Allow;
    }

    match requested_url.scheme() {
        "http" | "https" => match instance_origin {
            Some(origin) if same_origin(origin, requested_url) => Navigation::Allow,
            _ => Navigation::OpenExternally,
        },
        _ => Navigation::Block,
    }
}

/// Scheme, host, and port all match, which is exactly the boundary the
/// instance's `nxs_session` cookie is scoped to.
pub fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host() == right.host()
        && left.port_or_known_default() == right.port_or_known_default()
}

/// The shell's own bundled pages. Tauri serves them over the `tauri:` custom
/// protocol on macOS and Linux and over `http://tauri.localhost` on Windows, so
/// both spellings have to be recognised or the shell would try to open its own
/// connect page in the system browser.
fn is_shell_asset(url: &Url) -> bool {
    if matches!(url.scheme(), "tauri" | "asset") {
        return true;
    }
    if url.as_str() == "about:blank" {
        return true;
    }
    matches!(url.scheme(), "http" | "https")
        && matches!(
            url.host_str().map(str::to_ascii_lowercase).as_deref(),
            Some("tauri.localhost") | Some("asset.localhost")
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).expect("test url should parse")
    }

    fn decide(instance: &str, requested: &str) -> Navigation {
        policy(Some(&url(instance)), &url(requested))
    }

    #[test]
    fn the_instance_origin_is_allowed_at_any_depth() {
        let instance = "https://nexus.tail-net.ts.net/";
        assert_eq!(
            decide(instance, "https://nexus.tail-net.ts.net/"),
            Navigation::Allow
        );
        assert_eq!(
            decide(instance, "https://nexus.tail-net.ts.net/lists/7"),
            Navigation::Allow
        );
        assert_eq!(
            decide(
                instance,
                "https://nexus.tail-net.ts.net/vault/a/b/c.md?rev=3#top"
            ),
            Navigation::Allow
        );
    }

    #[test]
    fn the_login_redirect_is_allowed() {
        assert_eq!(
            decide(
                "https://nexus.tail-net.ts.net/",
                "https://nexus.tail-net.ts.net/login?next=%2Flists%2F7"
            ),
            Navigation::Allow
        );
    }

    #[test]
    fn a_plain_http_instance_on_a_lan_is_allowed_on_its_own_origin() {
        let instance = "http://192.168.0.183:8080/";
        assert_eq!(
            decide(instance, "http://192.168.0.183:8080/tasks"),
            Navigation::Allow
        );
        // A different port is a different origin, so a different cookie jar.
        assert_eq!(
            decide(instance, "http://192.168.0.183:9090/tasks"),
            Navigation::OpenExternally
        );
        // An upgrade to https is a different origin too, and the shell does not
        // silently follow one.
        assert_eq!(
            decide(instance, "https://192.168.0.183:8080/tasks"),
            Navigation::OpenExternally
        );
    }

    #[test]
    fn another_web_origin_goes_to_the_system_browser() {
        let instance = "https://nexus.tail-net.ts.net/";
        assert_eq!(
            decide(instance, "https://github.com/septimas7"),
            Navigation::OpenExternally
        );
        assert_eq!(
            decide(instance, "http://example.com/"),
            Navigation::OpenExternally
        );
        // A subdomain is still a different origin.
        assert_eq!(
            decide(instance, "https://cdn.nexus.tail-net.ts.net/x.js"),
            Navigation::OpenExternally
        );
    }

    #[test]
    fn everything_that_is_not_the_web_is_blocked() {
        let instance = "https://nexus.tail-net.ts.net/";
        for requested in [
            "mailto:someone@example.com",
            "file:///C:/Windows/System32/cmd.exe",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "ms-settings:privacy",
            "smb://fileserver/share",
        ] {
            assert_eq!(
                decide(instance, requested),
                Navigation::Block,
                "requested: {requested}"
            );
        }
    }

    #[test]
    fn the_shells_own_pages_are_always_allowed() {
        let instance = "https://nexus.tail-net.ts.net/";
        for requested in [
            "tauri://localhost/connect.html",
            "http://tauri.localhost/unreachable.html",
            "https://tauri.localhost/updating.html",
            "about:blank",
        ] {
            assert_eq!(
                decide(instance, requested),
                Navigation::Allow,
                "requested: {requested}"
            );
        }
        // And before any instance is bound, too.
        assert_eq!(
            policy(None, &url("tauri://localhost/connect.html")),
            Navigation::Allow
        );
    }

    #[test]
    fn with_no_instance_bound_the_web_still_goes_outside() {
        assert_eq!(
            policy(None, &url("https://github.com/")),
            Navigation::OpenExternally
        );
        assert_eq!(policy(None, &url("mailto:a@b.c")), Navigation::Block);
    }

    #[test]
    fn same_origin_ignores_the_default_port_spelling() {
        assert!(same_origin(
            &url("https://nexus.example.com/"),
            &url("https://nexus.example.com:443/x")
        ));
        assert!(same_origin(
            &url("http://nexus.example.com:80/"),
            &url("http://nexus.example.com/x")
        ));
        assert!(!same_origin(
            &url("https://nexus.example.com/"),
            &url("https://nexus.example.com:8443/")
        ));
    }
}
