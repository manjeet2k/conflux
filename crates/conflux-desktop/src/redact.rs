//! Pure text scrubbing for logs and diagnostics: URL credentials/queries, IP addresses and
//! file paths. Std-only so it is trivially testable.

use std::net::{Ipv4Addr, Ipv6Addr};

/// Characters that end a URL embedded in free text.
fn ends_url(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | ')' | '>' | '<' | ',' | '`')
}

/// Strips userinfo (`user:pass@`), query and fragment from one URL, keeping scheme, host,
/// port and path. A removed query/fragment is marked with `?<redacted>`.
pub fn redact_url(url: &str) -> String {
    let (scheme, rest) = match url.find("://") {
        Some(i) => (&url[..i + 3], &url[i + 3..]),
        None => ("", url),
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (path, had_extra) = match tail.find(['?', '#']) {
        Some(i) => (&tail[..i], true),
        None => (tail, false),
    };
    let marker = if had_extra { "?<redacted>" } else { "" };
    format!("{scheme}{host}{path}{marker}")
}

/// Redacts every `scheme://...` URL found in `text` (see [`redact_url`]).
pub fn redact_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("://") {
        // Walk back over the scheme (letters, digits, + - .).
        let before = &rest[..pos];
        let scheme_start = before
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
            .map_or(0, |i| i + 1);
        let url_end = rest[pos..].find(ends_url).map_or(rest.len(), |i| pos + i);
        out.push_str(&rest[..scheme_start]);
        out.push_str(&redact_url(&rest[scheme_start..url_end]));
        rest = &rest[url_end..];
    }
    out.push_str(rest);
    out
}

/// `192.168.1.57` -> `192.168.1.x`; IPv6 -> `<ipv6>`; `None` if `ip` is not an IP address.
pub fn mask_ip(ip: &str) -> Option<String> {
    if let Ok(v4) = ip.parse::<Ipv4Addr>() {
        let o = v4.octets();
        return Some(format!("{}.{}.{}.x", o[0], o[1], o[2]));
    }
    ip.parse::<Ipv6Addr>().ok().map(|_| "<ipv6>".to_string())
}

/// Masks every IPv4 / IPv6 address in `text`.
pub fn mask_ips(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_hexdigit() || c == ':' || c == '.' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_hexdigit() || chars[i] == ':' || chars[i] == '.')
            {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            // Trailing '.' or ':' is punctuation ("... 10.0.0.1."), not part of the address.
            let trimmed = token.trim_end_matches(['.', ':']);
            let suffix = &token[trimmed.len()..];
            // "10.0.0.1:443": mask the address, keep the port.
            let masked = mask_ip(trimmed).or_else(|| {
                let (host, port) = trimmed.split_once(':')?;
                let masked = mask_ip(host).filter(|m| m.ends_with(".x"))?;
                port.chars()
                    .all(|c| c.is_ascii_digit())
                    .then(|| format!("{masked}:{port}"))
            });
            match masked {
                Some(masked) => {
                    out.push_str(&masked);
                    out.push_str(suffix);
                }
                None => out.push_str(&token),
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Where a file path starts in `line`, if any: a Windows drive path (`C:\`, `C:/`), a UNC
/// path (`\\host`), or an absolute Unix path (`/` after whitespace, a quote or `=`).
fn find_path_start(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let prev = if i == 0 { b' ' } else { bytes[i - 1] };
        let boundary = prev.is_ascii_whitespace() || matches!(prev, b'"' | b'\'' | b'=' | b'(');
        if !boundary {
            continue;
        }
        if b.is_ascii_alphabetic()
            && i + 2 < bytes.len()
            && bytes[i + 1] == b':'
            && matches!(bytes[i + 2], b'\\' | b'/')
        {
            return Some(i);
        }
        if b == b'\\' && bytes.get(i + 1) == Some(&b'\\') {
            return Some(i);
        }
        // "/x" but not "//" (URL remnants) or a lone "/".
        if b == b'/'
            && bytes
                .get(i + 1)
                .is_some_and(|n| n.is_ascii_alphanumeric() || *n == b'.')
        {
            return Some(i);
        }
    }
    None
}

/// Replaces file paths with `<path>`. A path inside quotes ends at the closing quote;
/// otherwise (spaces are ambiguous, and a user name may contain one) it runs to the end of
/// the line, which can only over-redact.
pub fn redact_paths(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(start) = find_path_start(rest) {
        out.push_str(&rest[..start]);
        out.push_str("<path>");
        let quote = rest[..start]
            .chars()
            .last()
            .filter(|c| matches!(c, '"' | '\''));
        let after = &rest[start..];
        match quote.and_then(|q| after.find(q)) {
            Some(end) => rest = &after[end..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Everything a diagnostics line must lose before it leaves the machine.
pub fn scrub_diagnostic_line(line: &str) -> String {
    mask_ips(&redact_paths(&redact_urls(line)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials_and_query() {
        assert_eq!(
            redact_url("https://user:pw@example.com:8443/a/b.iso?token=abc&x=1#frag"),
            "https://example.com:8443/a/b.iso?<redacted>"
        );
        assert_eq!(
            redact_url("https://example.com/a/b.iso"),
            "https://example.com/a/b.iso"
        );
        assert_eq!(redact_url("https://u@example.com"), "https://example.com");
        assert_eq!(
            redact_url("https://example.com?t=1"),
            "https://example.com?<redacted>"
        );
        // '@' in the path is not userinfo.
        assert_eq!(
            redact_url("https://example.com/a@b"),
            "https://example.com/a@b"
        );
    }

    #[test]
    fn redacts_urls_inside_text() {
        let line = "WARN probe failed url=https://u:p@host.test/f?token=SECRET next";
        let out = redact_urls(line);
        assert_eq!(
            out,
            "WARN probe failed url=https://host.test/f?<redacted> next"
        );
        assert!(!out.contains("SECRET") && !out.contains("u:p"));

        let err = "error sending request for url (http://h/x?sig=1): timed out";
        assert_eq!(
            redact_urls(err),
            "error sending request for url (http://h/x?<redacted>): timed out"
        );
        let two = r#"a "ftp://a:b@h1/p?q" and https://h2/z?k=v."#;
        assert_eq!(
            redact_urls(two),
            r#"a "ftp://h1/p?<redacted>" and https://h2/z?<redacted>"#
        );
        assert_eq!(redact_urls("no urls here"), "no urls here");
        assert_eq!(redact_urls("odd :// alone"), "odd :// alone");
    }

    #[test]
    fn masks_ip_addresses() {
        assert_eq!(mask_ip("192.168.1.57").as_deref(), Some("192.168.1.x"));
        assert_eq!(mask_ip("fe80::1").as_deref(), Some("<ipv6>"));
        assert_eq!(mask_ip("example.com"), None);
        assert_eq!(
            mask_ips("bound 10.44.2.9:443 via fe80::4022:7688:97e1:1 and 1.2.3.4."),
            "bound 10.44.2.x:443 via <ipv6> and 1.2.3.x."
        );
        // Words and hex-looking identifiers are untouched.
        assert_eq!(mask_ips("decade face 1.5 MB"), "decade face 1.5 MB");
    }

    #[test]
    fn redacts_paths() {
        assert_eq!(
            redact_paths(r"Cannot create C:\Users\John Smith\Downloads\f.bin: denied"),
            "Cannot create <path>"
        );
        assert_eq!(
            redact_paths(r#"settings_path=Some("C:\\Users\\bob\\x.json") ok"#),
            r#"settings_path=Some("<path>") ok"#
        );
        assert_eq!(
            redact_paths("saved to /home/bob/file.iso"),
            "saved to <path>"
        );
        assert_eq!(redact_paths("wrote \\\\server\\share\\f"), "wrote <path>");
        assert_eq!(redact_paths("ratio 1/2 and a/b"), "ratio 1/2 and a/b");
        assert_eq!(redact_paths("https://h/x"), "https://h/x");
    }

    #[test]
    fn diagnostic_line_has_no_user_data() {
        let line = r"WARN failed https://u:p@h.test/f?t=1 from 192.168.1.57 path=C:\Users\bob\Downloads\a.bin";
        let out = scrub_diagnostic_line(line);
        for secret in ["u:p", "t=1", "57", "bob", "Users"] {
            assert!(!out.contains(secret), "{secret} leaked: {out}");
        }
    }
}
