//! Page Loader
//!
//! Loads pages that do not come from the network (`about:` and `file:`
//! URLs), plus a simple blocking HTTP fetch for tools and tests.

use crate::page::Page;
use std::error::Error;
use std::path::{Path, PathBuf};

/// Page loader
pub struct Loader {
    /// User agent string
    user_agent: String,
}

impl Loader {
    /// Create a new loader
    pub fn new() -> Self {
        Self {
            user_agent: "Mozilla/5.0 (compatible; fOS-Browser/0.1; +https://github.com/fosproject)".to_string(),
        }
    }

    /// Whether `url` is loaded locally rather than over the network
    pub fn is_local_url(url: &str) -> bool {
        let lower = url.trim_start().to_ascii_lowercase();
        lower.starts_with("about:") || lower.starts_with("file:")
    }

    /// Load a page from URL (blocking)
    pub fn load_sync(&self, url: &str) -> Result<Page, Box<dyn Error>> {
        // Handle about: URLs
        if url.starts_with("about:") {
            return Ok(self.load_about_page(url));
        }

        // Handle local files
        if url.to_ascii_lowercase().starts_with("file:") {
            let html = self.load_file(url)?;
            return Ok(Page::from_html(url, html));
        }

        // Fetch HTML
        let html = self.fetch_html(url)?;

        // Create page
        let page = Page::from_html(url, html);

        Ok(page)
    }

    /// Fetch HTML content
    fn fetch_html(&self, url: &str) -> Result<String, Box<dyn Error>> {
        // Use custom blocking client from fos-net
        let mut client = fos_net::client::blocking::Client::builder()
            .user_agent(&self.user_agent)
            .build()?;

        let response = client.get(url)?;

        if !response.is_success() {
            return Err(format!("HTTP error: {}", response.status).into());
        }

        let content_type = response.header("content-type").map(str::to_owned);
        Ok(crate::charset::decode_html(response.body, content_type.as_deref()))
    }

    /// Load a `file:` URL: HTML and text files are shown, directories listed
    fn load_file(&self, url: &str) -> Result<String, Box<dyn Error>> {
        let path = file_url_to_path(url).ok_or_else(|| format!("Invalid file URL: {}", url))?;

        if path.is_dir() {
            return Ok(directory_listing(&path)?);
        }

        let bytes = std::fs::read(&path)
            .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;

        let is_html = path.extension()
            .map(|ext| {
                let ext = ext.to_string_lossy().to_ascii_lowercase();
                matches!(ext.as_str(), "html" | "htm" | "xhtml" | "svg" | "xml")
            })
            .unwrap_or(false);

        let text = crate::charset::decode_html(bytes, None);
        if is_html {
            Ok(text)
        } else {
            Ok(format!("<html><body><pre>{}</pre></body></html>", escape_html(&text)))
        }
    }

    /// Load an about: page
    fn load_about_page(&self, url: &str) -> Page {
        let html = match url {
            "about:blank" => r#"
                <!DOCTYPE html>
                <html>
                <head><title>New Tab</title></head>
                <body style="background: #0d0d0d; color: #e0e0e0; font-family: sans-serif;">
                </body>
                </html>
            "#.to_string(),

            "about:version" => format!(r#"
                <!DOCTYPE html>
                <html>
                <head><title>fOS Browser</title></head>
                <body style="background: #0d0d0d; color: #e0e0e0; font-family: sans-serif; padding: 20px;">
                    <h1>fOS Browser</h1>
                    <p>Version: {}</p>
                    <p>Engine: fOS Engine</p>
                    <p>Built with Rust</p>
                </body>
                </html>
            "#, env!("CARGO_PKG_VERSION")),

            _ => format!(r#"
                <!DOCTYPE html>
                <html>
                <head><title>Unknown Page</title></head>
                <body style="background: #0d0d0d; color: #e0e0e0; font-family: sans-serif; padding: 20px;">
                    <h1>Unknown about: page</h1>
                    <p>The page <code>{}</code> was not found.</p>
                </body>
                </html>
            "#, escape_html(url)),
        };

        Page::from_html(url, html)
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a `file:` URL to a local path (percent-decoding it)
fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.get(5..)?; // after "file:"
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    // Drop an explicit "localhost" authority
    let rest = match rest.get(..9) {
        Some(prefix) if prefix.eq_ignore_ascii_case("localhost") => &rest[9..],
        _ => rest,
    };
    let rest = rest.split(['?', '#']).next().unwrap_or("");

    // Percent-decode
    let bytes = rest.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16);
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                decoded.push((high * 16 + low) as u8);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let path = String::from_utf8(decoded).ok()?;

    // Windows drive paths come as "/C:/..."
    #[cfg(windows)]
    let path = path.strip_prefix('/').filter(|p| p.get(1..2) == Some(":")).map(str::to_owned).unwrap_or(path);

    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

/// HTML listing of a directory
fn directory_listing(dir: &Path) -> std::io::Result<String> {
    let mut entries: Vec<(String, bool)> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path().is_dir()))
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));

    let title = escape_html(&dir.display().to_string());
    let mut html = format!("<html><head><title>{0}</title></head><body><h1>Index of {0}</h1><ul>", title);
    if dir.parent().is_some() {
        html.push_str("<li><a href=\"../\">../</a></li>");
    }
    for (name, is_dir) in entries {
        let suffix = if is_dir { "/" } else { "" };
        let href = encode_path_segment(&name);
        html.push_str(&format!(
            "<li><a href=\"{}{}\">{}{}</a></li>",
            href, suffix, escape_html(&name), suffix
        ));
    }
    html.push_str("</ul></body></html>");
    Ok(html)
}

fn encode_path_segment(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for b in name.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_about_blank() {
        let loader = Loader::new();
        let page = loader.load_about_page("about:blank");

        assert_eq!(page.title, Some("New Tab".to_string()));
    }

    #[test]
    fn test_about_version() {
        let loader = Loader::new();
        let page = loader.load_about_page("about:version");

        assert_eq!(page.title, Some("fOS Browser".to_string()));
    }

    #[test]
    fn test_is_local_url() {
        assert!(Loader::is_local_url("about:blank"));
        assert!(Loader::is_local_url("file:///tmp/x.html"));
        assert!(!Loader::is_local_url("https://example.com"));
    }

    #[test]
    fn test_file_url_to_path() {
        assert_eq!(file_url_to_path("file:///tmp/a%20b.html"), Some(PathBuf::from("/tmp/a b.html")));
        assert_eq!(file_url_to_path("file://localhost/etc/hosts#x"), Some(PathBuf::from("/etc/hosts")));
        assert_eq!(file_url_to_path("file://"), None);
    }

    #[test]
    fn test_load_file_and_directory() {
        let dir = std::env::temp_dir().join(format!("fos-loader-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("page.html"), "<title>Local</title><p>hi</p>").unwrap();
        std::fs::write(dir.join("notes.txt"), "a < b").unwrap();

        let loader = Loader::new();
        let base = format!("file://{}", dir.display());

        let page = loader.load_sync(&format!("{}/page.html", base)).unwrap();
        assert_eq!(page.title, Some("Local".to_string()));

        let text = loader.load_sync(&format!("{}/notes.txt", base)).unwrap();
        assert!(text.html.contains("a &lt; b"));

        let listing = loader.load_sync(&base).unwrap();
        assert!(listing.html.contains("page.html"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
