//! Import maps (`<script type="importmap">`)
//!
//! Resolving the specifiers of module imports: relative references and
//! URLs resolve against the importing module, and bare names ("vue",
//! "lodash/") go through the page's import maps, globally or per scope,
//! as in HTML's "resolve a module specifier".

use fos_net::url_util;

/// Specifier map entries: (key, address), keys in descending order so
/// longer prefixes are tried first. A `None` address blocks the key.
type SpecifierMap = Vec<(String, Option<String>)>;

#[derive(Debug, Default, Clone)]
pub struct ImportMap {
    imports: SpecifierMap,
    /// (scope prefix URL, map), longest prefixes first
    scopes: Vec<(String, SpecifierMap)>,
}

impl ImportMap {
    /// Merge the import map `json` of the page at `base` into this one.
    /// Rules already present win over new ones for the same key.
    pub fn add(&mut self, json: &str, base: &str) -> Result<(), String> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Invalid import map: {e}"))?;
        let map = value.as_object().ok_or("An import map must be a JSON object")?;
        if let Some(imports) = map.get("imports") {
            let parsed = specifier_map(imports, base)?;
            merge(&mut self.imports, parsed);
        }
        if let Some(scopes) = map.get("scopes") {
            let scopes = scopes.as_object().ok_or("An import map's \"scopes\" must be an object")?;
            for (prefix, entries) in scopes {
                let prefix = url_util::resolve(base, prefix);
                let parsed = specifier_map(entries, base)?;
                match self.scopes.iter_mut().find(|(p, _)| *p == prefix) {
                    Some((_, existing)) => merge(existing, parsed),
                    None => self.scopes.push((prefix, parsed)),
                }
            }
            self.scopes.sort_by(|a, b| b.0.cmp(&a.0));
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.imports.is_empty() && self.scopes.is_empty()
    }

    /// The URL of `specifier` imported by the module (or page) at `base`
    pub fn resolve(&self, specifier: &str, base: &str) -> Result<String, String> {
        let as_url = url_like(specifier, base);
        let normalized = as_url.as_deref().unwrap_or(specifier);
        for (prefix, map) in &self.scopes {
            if prefix == base || (prefix.ends_with('/') && base.starts_with(prefix.as_str())) {
                if let Some(url) = resolve_in(normalized, as_url.as_deref(), map)? {
                    return Ok(url);
                }
            }
        }
        if let Some(url) = resolve_in(normalized, as_url.as_deref(), &self.imports)? {
            return Ok(url);
        }
        as_url.ok_or_else(|| {
            format!("Failed to resolve module specifier \"{specifier}\". Relative references must start with \"/\", \"./\", or \"../\".")
        })
    }
}

/// A specifier that is a URL or a relative reference, as a URL
fn url_like(specifier: &str, base: &str) -> Option<String> {
    if specifier.starts_with('/') || specifier.starts_with("./") || specifier.starts_with("../") {
        return Some(url_util::resolve(base, specifier));
    }
    let (scheme, _) = specifier.split_once(':')?;
    let mut chars = scheme.chars();
    let is_scheme = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    is_scheme.then(|| url_util::resolve(base, specifier))
}

fn is_special(url: &str) -> bool {
    matches!(url.split_once(':').map(|(s, _)| s.to_ascii_lowercase()).as_deref(), Some("http" | "https" | "ftp" | "file" | "ws" | "wss"))
}

fn resolve_in(normalized: &str, as_url: Option<&str>, map: &SpecifierMap) -> Result<Option<String>, String> {
    let blocked = || Err(format!("Import of \"{normalized}\" is blocked by the import map"));
    for (key, address) in map {
        if key == normalized {
            return match address {
                Some(a) => Ok(Some(a.clone())),
                None => blocked(),
            };
        }
        // Prefix rules ("lodash/" -> ".../lodash-es/")
        if key.ends_with('/') && normalized.starts_with(key.as_str()) && as_url.is_none_or(is_special) {
            let Some(address) = address else { return blocked() };
            let url = url_util::resolve(address, &normalized[key.len()..]);
            if !url.starts_with(address.as_str()) {
                return Err(format!("Import of \"{normalized}\" escapes its import map prefix"));
            }
            return Ok(Some(url));
        }
    }
    Ok(None)
}

fn specifier_map(value: &serde_json::Value, base: &str) -> Result<SpecifierMap, String> {
    let entries = value.as_object().ok_or("An import map's specifier map must be an object")?;
    let mut map = SpecifierMap::new();
    for (key, address) in entries {
        if key.is_empty() {
            continue;
        }
        let key = url_like(key, base).unwrap_or_else(|| key.clone());
        let address = address
            .as_str()
            .and_then(|a| url_like(a, base))
            // A prefix rule must map to a prefix
            .filter(|a| !key.ends_with('/') || a.ends_with('/'));
        if address.is_none() {
            log::warn!("Import map entry \"{key}\" has an invalid address; imports of it are blocked");
        }
        map.push((key, address));
    }
    map.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(map)
}

/// Add `new` rules to `existing`, keeping the existing rule for a key
fn merge(existing: &mut SpecifierMap, new: SpecifierMap) {
    for (key, address) in new {
        if existing.iter().any(|(k, _)| *k == key) {
            log::warn!("Import map rule for \"{key}\" ignored: an earlier map defines it");
            continue;
        }
        existing.push((key, address));
    }
    existing.sort_by(|a, b| b.0.cmp(&a.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "https://example.com/app/index.html";

    #[test]
    fn relative_and_absolute_specifiers() {
        let map = ImportMap::default();
        assert_eq!(map.resolve("./a.js", "https://example.com/app/main.js").unwrap(), "https://example.com/app/a.js");
        assert_eq!(map.resolve("../lib/b.js", "https://example.com/app/x/y.js").unwrap(), "https://example.com/app/lib/b.js");
        assert_eq!(map.resolve("/root.js", PAGE).unwrap(), "https://example.com/root.js");
        assert_eq!(map.resolve("https://cdn.example/m.js", PAGE).unwrap(), "https://cdn.example/m.js");
        assert!(map.resolve("vue", PAGE).unwrap_err().contains("Failed to resolve module specifier \"vue\""));
        assert!(map.resolve("a.js", PAGE).is_err());
    }

    #[test]
    fn bare_names_and_prefixes() {
        let mut map = ImportMap::default();
        map.add(
            r#"{
              "imports": {
                "vue": "https://cdn.example/vue@3/dist/vue.esm-browser.js",
                "lodash/": "/vendor/lodash-es/",
                "lodash/fp": "./vendor/fp.js",
                "app/": "./src/",
                "blocked": null,
                "https://old.example/lib.js": "/lib.js"
              },
              "scopes": {
                "/legacy/": { "vue": "https://cdn.example/vue@2/vue.js" }
              }
            }"#,
            PAGE,
        )
        .unwrap();
        assert_eq!(map.resolve("vue", PAGE).unwrap(), "https://cdn.example/vue@3/dist/vue.esm-browser.js");
        assert_eq!(map.resolve("lodash/debounce.js", PAGE).unwrap(), "https://example.com/vendor/lodash-es/debounce.js");
        // The exact rule wins over the prefix rule
        assert_eq!(map.resolve("lodash/fp", PAGE).unwrap(), "https://example.com/app/vendor/fp.js");
        assert_eq!(map.resolve("app/util/x.js", "https://example.com/app/src/main.js").unwrap(), "https://example.com/app/src/util/x.js");
        assert!(map.resolve("blocked", PAGE).is_err());
        // URLs can be remapped too
        assert_eq!(map.resolve("https://old.example/lib.js", PAGE).unwrap(), "https://example.com/lib.js");
        // Scopes apply to modules under their prefix
        assert_eq!(map.resolve("vue", "https://example.com/legacy/widget.js").unwrap(), "https://cdn.example/vue@2/vue.js");
        // A prefix rule cannot be escaped
        assert!(map.resolve("lodash/../../secret.js", PAGE).is_err());
    }

    #[test]
    fn several_maps_merge() {
        let mut map = ImportMap::default();
        map.add(r#"{"imports": {"a": "/a1.js"}}"#, PAGE).unwrap();
        map.add(r#"{"imports": {"a": "/a2.js", "b": "/b.js"}}"#, PAGE).unwrap();
        assert_eq!(map.resolve("a", PAGE).unwrap(), "https://example.com/a1.js");
        assert_eq!(map.resolve("b", PAGE).unwrap(), "https://example.com/b.js");
        assert!(map.add("not json", PAGE).is_err());
        assert!(map.add("[1]", PAGE).is_err());
    }
}
