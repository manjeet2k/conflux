//! File name derivation: Content-Disposition parsing, URL fallback, and Windows-safe sanitizing.

use std::path::{Path, PathBuf};

/// Fallback name used whenever nothing usable can be derived.
pub const DEFAULT_FILENAME: &str = "download.bin";

/// Maximum length (in bytes) of a sanitized file name.
const MAX_FILENAME_BYTES: usize = 200;

/// Characters that are invalid in Windows file names (plus path separators).
const FORBIDDEN_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Windows device names, matched case-insensitively against the part before the first `.`.
/// Includes the superscript-digit forms (`COM¹`), which Windows also treats as devices.
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "CONIN$",
    "CONOUT$", "COM0", "LPT0", "COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³",
];

/// `true` for invisible Unicode format characters that can disguise a file name, e.g.
/// RIGHT-TO-LEFT OVERRIDE turning `invoice\u{202E}fdp.exe` into "invoiceexe.pdf" on screen.
///
/// Covered: soft hyphen (U+00AD), Arabic letter mark (U+061C), zero-width space/non-joiner/
/// joiner and LRM/RLM (U+200B-U+200F), bidi embeddings/overrides (U+202A-U+202E), word joiner
/// and invisible operators (U+2060-U+2064), bidi isolates (U+2066-U+2069), BOM (U+FEFF).
fn is_invisible_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

/// Decodes `%XX` escapes into raw bytes. Malformed escapes are kept literally.
pub(crate) fn percent_decode(input: &str) -> Vec<u8> {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }

    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Splits a Content-Disposition header into lowercase parameter names and raw values.
/// Handles quoted strings (with `\` escapes and embedded `;`) and unquoted tokens.
fn parse_disposition_params(header: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = header.chars().collect();
    let mut params = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        // Skip separators and whitespace.
        while i < chars.len() && (chars[i] == ';' || chars[i].is_whitespace()) {
            i += 1;
        }
        // Read the parameter name (or the bare disposition type).
        let name_start = i;
        while i < chars.len() && chars[i] != '=' && chars[i] != ';' {
            i += 1;
        }
        let name: String = chars[name_start..i].iter().collect();
        let name = name.trim().to_ascii_lowercase();

        if i >= chars.len() || chars[i] == ';' {
            // Bare token such as `attachment`; no value.
            continue;
        }
        i += 1; // skip '='
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }

        let mut value = String::new();
        if i < chars.len() && chars[i] == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                }
                value.push(chars[i]);
                i += 1;
            }
            // Skip closing quote and anything up to the next ';'.
            while i < chars.len() && chars[i] != ';' {
                i += 1;
            }
        } else {
            while i < chars.len() && chars[i] != ';' {
                value.push(chars[i]);
                i += 1;
            }
            value = value.trim().to_string();
        }

        if !name.is_empty() {
            params.push((name, value));
        }
    }

    params
}

/// Decodes an RFC 5987 ext-value: `charset'language'percent-encoded`.
fn decode_ext_value(value: &str) -> Option<String> {
    let mut parts = value.splitn(3, '\'');
    let charset = parts.next()?.trim();
    let _language = parts.next()?;
    let encoded = parts.next()?;
    let bytes = percent_decode(encoded);
    if charset.eq_ignore_ascii_case("utf-8") {
        String::from_utf8(bytes).ok()
    } else if charset.eq_ignore_ascii_case("iso-8859-1") {
        Some(bytes.into_iter().map(char::from).collect())
    } else {
        None
    }
}

/// Extracts the raw (unsanitized) file name from a Content-Disposition header value.
/// `filename*` (RFC 5987/6266) takes precedence over `filename`.
pub fn parse_content_disposition_filename(header: &str) -> Option<String> {
    let params = parse_disposition_params(header);

    let extended = params
        .iter()
        .filter(|(name, _)| name == "filename*")
        .find_map(|(_, value)| decode_ext_value(value))
        .filter(|v| !v.trim().is_empty());
    if extended.is_some() {
        return extended;
    }

    params
        .into_iter()
        .find(|(name, _)| name == "filename")
        .map(|(_, value)| value)
        .filter(|v| !v.trim().is_empty())
}

/// Returns the percent-decoded last non-empty path segment of `url`, if any.
pub fn filename_from_url(url: &reqwest::Url) -> Option<String> {
    let last = url.path_segments()?.next_back()?;
    if last.is_empty() {
        return None;
    }
    let decoded = String::from_utf8_lossy(&percent_decode(last)).into_owned();
    if decoded.trim().is_empty() {
        None
    } else {
        Some(decoded)
    }
}

/// Picks a raw file name from the Content-Disposition header (if present) or the URL,
/// then sanitizes it.
pub(crate) fn derive_filename(content_disposition: Option<&str>, url: &reqwest::Url) -> String {
    let raw = content_disposition
        .and_then(parse_content_disposition_filename)
        .or_else(|| filename_from_url(url))
        .unwrap_or_else(|| DEFAULT_FILENAME.to_string());
    sanitize_filename(&raw)
}

fn is_reserved_windows_name(name: &str) -> bool {
    // Windows treats `CON`, `con.txt` and `CON.tar.gz` (and `CON .txt`) as the device.
    let stem = name.split('.').next().unwrap_or("").trim_end();
    RESERVED_NAMES.iter().any(|r| stem.eq_ignore_ascii_case(r))
}

/// Largest byte index `<= max` that lies on a char boundary of `s`.
fn floor_char_boundary(s: &str, max: usize) -> usize {
    if max >= s.len() {
        return s.len();
    }
    let mut idx = max;
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn truncate_preserving_extension(name: &str, max_bytes: usize) -> String {
    if name.len() <= max_bytes {
        return name.to_string();
    }
    // Preserve a reasonably short extension (e.g. ".tar", ".iso").
    let ext = match name.rfind('.') {
        Some(idx) if idx > 0 && name.len() - idx <= 16 => &name[idx..],
        _ => "",
    };
    let stem = &name[..name.len() - ext.len()];
    let keep = floor_char_boundary(stem, max_bytes - ext.len());
    let stem = stem[..keep].trim_end_matches(['.', ' ']);
    if stem.is_empty() {
        let keep = floor_char_boundary(name, max_bytes);
        return name[..keep].to_string();
    }
    format!("{}{}", stem, ext)
}

/// Produces a bare, Windows-safe file name from an untrusted value (header or URL).
///
/// - keeps only the final component after any `/` or `\` (defeats path traversal)
/// - strips control characters, invisible bidi/zero-width format characters and `<>:"/\|?*`
/// - strips leading whitespace and trailing dots/spaces
/// - rejects `.`/`..`/empty (falls back to `download.bin`)
/// - prefixes `_` to Windows reserved device names (`CON`, `nul.txt`, `COM1`, ...)
/// - caps the length at ~200 bytes on a char boundary, preserving the extension
pub fn sanitize_filename(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last
        .chars()
        .filter(|c| {
            !c.is_control() && !is_invisible_format_char(*c) && !FORBIDDEN_CHARS.contains(c)
        })
        .collect();
    let cleaned = cleaned.trim_start().trim_end_matches(['.', ' ']);

    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return DEFAULT_FILENAME.to_string();
    }

    let mut name = cleaned.to_string();
    if is_reserved_windows_name(&name) {
        name.insert(0, '_');
    }

    let name = truncate_preserving_extension(&name, MAX_FILENAME_BYTES);
    if name.is_empty() {
        DEFAULT_FILENAME.to_string()
    } else {
        name
    }
}

/// Returns `dir/filename` if nothing exists there, otherwise the first of
/// `dir/"name (1).ext"`, `dir/"name (2).ext"`, ... that does not exist.
///
/// Existence is checked with `symlink_metadata`, so a dangling symlink counts as taken.
/// Note: this is a check-then-use helper; the caller creates the file afterwards.
fn unique_path(dir: &Path, filename: &str) -> PathBuf {
    let taken = |p: &Path| std::fs::symlink_metadata(p).is_ok();

    let candidate = dir.join(filename);
    if !taken(&candidate) {
        return candidate;
    }

    let as_path = Path::new(filename);
    let stem = as_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| filename.to_string());
    let ext = as_path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut n: u64 = 1;
    loop {
        let candidate = dir.join(format!("{} ({}){}", stem, n, ext));
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Like [`unique_path`], but atomically claims the name by creating an empty file there
/// (`create_new`), so two concurrent downloads (in this or another process) can never pick
/// the same path. The caller then opens the claimed file for writing.
pub fn claim_unique_path(dir: &Path, filename: &str) -> std::io::Result<PathBuf> {
    loop {
        let candidate = unique_path(dir, filename);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            // Someone took it between the check and the create: pick the next free name.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("a%20b"), b"a b");
        assert_eq!(percent_decode("%E2%82%AC"), "\u{20ac}".as_bytes());
        assert_eq!(percent_decode("100%"), b"100%");
        assert_eq!(percent_decode("%zz%4"), b"%zz%4");
        assert_eq!(percent_decode("%41%42"), b"AB");
        assert_eq!(percent_decode(""), b"");
    }

    #[test]
    fn test_cd_plain_quoted_and_unquoted() {
        assert_eq!(
            parse_content_disposition_filename("attachment; filename=\"report.pdf\""),
            Some("report.pdf".into())
        );
        assert_eq!(
            parse_content_disposition_filename("attachment; filename=report.pdf; size=123"),
            Some("report.pdf".into())
        );
        assert_eq!(
            parse_content_disposition_filename("attachment;filename=report.pdf"),
            Some("report.pdf".into())
        );
        assert_eq!(
            parse_content_disposition_filename("attachment; FILENAME = data.csv"),
            Some("data.csv".into())
        );
    }

    #[test]
    fn test_cd_quoted_with_semicolon_and_escapes() {
        assert_eq!(
            parse_content_disposition_filename(
                "attachment; filename=\"a; b.txt\"; creation-date=\"x\""
            ),
            Some("a; b.txt".into())
        );
        assert_eq!(
            parse_content_disposition_filename(r#"attachment; filename="say \"hi\".txt""#),
            Some("say \"hi\".txt".into())
        );
    }

    #[test]
    fn test_cd_extended_preferred() {
        assert_eq!(
            parse_content_disposition_filename(
                "attachment; filename=\"fallback.txt\"; filename*=UTF-8''%E2%82%AC%20rates.txt"
            ),
            Some("\u{20ac} rates.txt".into())
        );
        // Order must not matter.
        assert_eq!(
            parse_content_disposition_filename(
                "attachment; filename*=utf-8'en'na%C3%AFve.bin; filename=naive.bin"
            ),
            Some("na\u{ef}ve.bin".into())
        );
        assert_eq!(
            parse_content_disposition_filename("attachment; filename*=iso-8859-1''caf%E9.txt"),
            Some("caf\u{e9}.txt".into())
        );
        // Unsupported charset or invalid UTF-8 falls back to plain filename.
        assert_eq!(
            parse_content_disposition_filename(
                "attachment; filename*=UTF-8''%FF%FE; filename=plain.txt"
            ),
            Some("plain.txt".into())
        );
    }

    #[test]
    fn test_cd_without_filename() {
        assert_eq!(parse_content_disposition_filename("attachment"), None);
        assert_eq!(parse_content_disposition_filename("inline; size=5"), None);
        assert_eq!(
            parse_content_disposition_filename("attachment; filename=\"\""),
            None
        );
        assert_eq!(parse_content_disposition_filename(""), None);
    }

    #[test]
    fn test_derive_filename_fallbacks() {
        let url = reqwest::Url::parse("https://example.com/files/My%20Report.pdf?x=1").unwrap();
        assert_eq!(derive_filename(None, &url), "My Report.pdf");
        assert_eq!(derive_filename(Some("attachment"), &url), "My Report.pdf");
        assert_eq!(
            derive_filename(Some("attachment; filename=\"../../evil.sh\""), &url),
            "evil.sh"
        );
        let root = reqwest::Url::parse("https://example.com/").unwrap();
        assert_eq!(derive_filename(None, &root), "download.bin");
        assert_eq!(derive_filename(Some("attachment"), &root), "download.bin");
        let encoded_slash =
            reqwest::Url::parse("https://example.com/a/..%2F..%2Fetc%2Fpasswd").unwrap();
        assert_eq!(derive_filename(None, &encoded_slash), "passwd");
    }

    #[test]
    fn test_sanitize_traversal_and_paths() {
        assert_eq!(sanitize_filename("../../x"), "x");
        assert_eq!(sanitize_filename("C:\\Windows\\evil.exe"), "evil.exe");
        assert_eq!(sanitize_filename("/etc/passwd"), "passwd");
        assert_eq!(sanitize_filename(".."), "download.bin");
        assert_eq!(sanitize_filename("."), "download.bin");
        assert_eq!(sanitize_filename("dir/"), "download.bin");
        assert_eq!(sanitize_filename("C:evil.exe"), "Cevil.exe");
    }

    #[test]
    fn test_sanitize_forbidden_and_control_chars() {
        assert_eq!(sanitize_filename("a<b>c:d\"e|f?g*h.txt"), "abcdefgh.txt");
        assert_eq!(sanitize_filename("bad\u{0}\n\r\tname.bin"), "badname.bin");
        assert_eq!(sanitize_filename("  spaced.txt  "), "spaced.txt");
    }

    #[test]
    fn test_sanitize_trailing_dots_and_empty() {
        assert_eq!(sanitize_filename("file.txt..."), "file.txt");
        assert_eq!(sanitize_filename("file. . ."), "file");
        assert_eq!(sanitize_filename(""), "download.bin");
        assert_eq!(sanitize_filename("   "), "download.bin");
        assert_eq!(sanitize_filename("***"), "download.bin");
        assert_eq!(sanitize_filename("..."), "download.bin");
    }

    #[test]
    fn test_sanitize_reserved_names() {
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename("con.tar.gz"), "_con.tar.gz");
        assert_eq!(sanitize_filename("nul"), "_nul");
        assert_eq!(sanitize_filename("Com1.log"), "_Com1.log");
        assert_eq!(sanitize_filename("lpt9"), "_lpt9");
        assert_eq!(sanitize_filename("CON .txt"), "_CON .txt");
        // Not reserved.
        assert_eq!(sanitize_filename("CONSOLE.txt"), "CONSOLE.txt");
        assert_eq!(sanitize_filename("COM10.txt"), "COM10.txt");
        assert_eq!(sanitize_filename("xCON.txt"), "xCON.txt");
    }

    #[test]
    fn test_sanitize_more_reserved_device_names() {
        for name in [
            "CONIN$",
            "conin$.txt",
            "CONOUT$",
            "Conout$.log",
            "COM0",
            "com0.bin",
            "LPT0",
            "lpt0.txt",
            "COM\u{b9}",
            "COM\u{b2}.txt",
            "com\u{b3}",
            "LPT\u{b9}.doc",
            "lpt\u{b2}",
            "LPT\u{b3}",
        ] {
            assert_eq!(sanitize_filename(name), format!("_{}", name), "{:?}", name);
        }
        // Not reserved.
        assert_eq!(sanitize_filename("CONIN.txt"), "CONIN.txt");
        assert_eq!(sanitize_filename("COM\u{b9}0.txt"), "COM\u{b9}0.txt");
        assert_eq!(sanitize_filename("LPT00"), "LPT00");
    }

    #[test]
    fn test_sanitize_strips_invisible_format_chars() {
        // RIGHT-TO-LEFT OVERRIDE would render "invoice\u{202E}fdp.exe" as "invoiceexe.pdf".
        assert_eq!(
            sanitize_filename("invoice\u{202E}fdp.exe"),
            "invoicefdp.exe"
        );
        let invisible = [
            '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}',
            '\u{2068}', '\u{2069}', '\u{200E}', '\u{200F}', '\u{061C}', '\u{200B}', '\u{200C}',
            '\u{200D}', '\u{2060}', '\u{2061}', '\u{2062}', '\u{2063}', '\u{2064}', '\u{FEFF}',
            '\u{00AD}',
        ];
        for c in invisible {
            let raw = format!("{c}re{c}port{c}.pdf{c}");
            assert_eq!(sanitize_filename(&raw), "report.pdf", "U+{:04X}", c as u32);
        }
        // A name made only of invisible characters falls back to the default.
        assert_eq!(sanitize_filename("\u{200B}\u{FEFF}"), "download.bin");
        // Visible non-ASCII is untouched.
        assert_eq!(sanitize_filename("日本語.txt"), "日本語.txt");
    }

    #[test]
    fn test_sanitize_unicode_and_length_cap() {
        assert_eq!(sanitize_filename("résumé 日本語.pdf"), "résumé 日本語.pdf");

        let long = format!("{}.iso", "é".repeat(300));
        let out = sanitize_filename(&long);
        assert!(out.len() <= 200, "len {}", out.len());
        assert!(out.ends_with(".iso"));
        assert!(out.starts_with('é'));

        let long_no_ext = "x".repeat(500);
        assert_eq!(sanitize_filename(&long_no_ext).len(), 200);

        let emoji = format!("{}.txt", "\u{1F600}".repeat(100));
        let out = sanitize_filename(&emoji);
        assert!(out.len() <= 200);
        assert!(out.ends_with(".txt"));
    }

    #[test]
    fn test_sanitize_is_idempotent_and_never_empty() {
        let inputs = [
            "../../x",
            "C:\\Windows\\evil.exe",
            "CON.txt",
            "a..",
            "",
            "résumé.pdf",
            "\u{7f}",
            "normal-file_v1.2.tar.gz",
        ];
        for input in inputs {
            let once = sanitize_filename(input);
            assert!(!once.is_empty());
            assert!(!once.contains(['/', '\\']));
            assert_eq!(
                sanitize_filename(&once),
                once,
                "not idempotent for {:?}",
                input
            );
        }
    }

    #[test]
    fn test_claim_unique_path_creates_distinct_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let a = claim_unique_path(d, "f.bin").unwrap();
        let b = claim_unique_path(d, "f.bin").unwrap();
        assert_eq!(a, d.join("f.bin"));
        assert_eq!(b, d.join("f (1).bin"));
        assert!(a.is_file() && b.is_file());
        assert!(claim_unique_path(&d.join("missing"), "f.bin").is_err());
    }

    #[test]
    fn test_unique_path() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();

        assert_eq!(unique_path(d, "file.txt"), d.join("file.txt"));
        std::fs::write(d.join("file.txt"), b"").unwrap();
        assert_eq!(unique_path(d, "file.txt"), d.join("file (1).txt"));
        std::fs::write(d.join("file (1).txt"), b"").unwrap();
        std::fs::write(d.join("file (2).txt"), b"").unwrap();
        assert_eq!(unique_path(d, "file.txt"), d.join("file (3).txt"));

        std::fs::write(d.join("README"), b"").unwrap();
        assert_eq!(unique_path(d, "README"), d.join("README (1)"));

        std::fs::write(d.join("a.tar.gz"), b"").unwrap();
        assert_eq!(unique_path(d, "a.tar.gz"), d.join("a.tar (1).gz"));

        // Directories count as taken too.
        std::fs::create_dir(d.join("folder")).unwrap();
        assert_eq!(unique_path(d, "folder"), d.join("folder (1)"));
    }
}
