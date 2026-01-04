//! Application Panel
//!
//! Service Worker status, manifest viewer, and PWA installation status.

/// Service Worker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceWorkerState { Parsed, Installing, Installed, Activating, Activated, Redundant }

/// Service Worker info
#[derive(Debug, Clone)]
pub struct ServiceWorkerInfo {
    pub script_url: String,
    pub scope: String,
    pub state: ServiceWorkerState,
    pub running: bool,
    pub fetch_count: u32,
    pub last_update: Option<u64>,
    pub version_id: String,
}

/// Web App Manifest
#[derive(Debug, Clone, Default)]
pub struct WebAppManifest {
    pub name: Option<String>,
    pub short_name: Option<String>,
    pub description: Option<String>,
    pub start_url: Option<String>,
    pub scope: Option<String>,
    pub display: DisplayMode,
    pub orientation: Orientation,
    pub theme_color: Option<String>,
    pub background_color: Option<String>,
    pub icons: Vec<ManifestIcon>,
    pub categories: Vec<String>,
    pub lang: Option<String>,
}

/// Display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode { #[default] Browser, Standalone, MinimalUi, Fullscreen, WindowControlsOverlay }

impl DisplayMode {
    pub fn parse(s: &str) -> Self {
        match s { "standalone" => Self::Standalone, "minimal-ui" => Self::MinimalUi,
                  "fullscreen" => Self::Fullscreen, "window-controls-overlay" => Self::WindowControlsOverlay,
                  _ => Self::Browser }
    }
}

/// Orientation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation { #[default] Any, Natural, Landscape, Portrait, LandscapePrimary, PortraitPrimary }

/// Manifest icon
#[derive(Debug, Clone)]
pub struct ManifestIcon {
    pub src: String,
    pub sizes: Vec<(u32, u32)>,
    pub icon_type: Option<String>,
    pub purpose: IconPurpose,
}

/// Icon purpose
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconPurpose { #[default] Any, Monochrome, Maskable }

/// PWA installation status
#[derive(Debug, Clone)]
pub struct PwaStatus {
    pub installable: bool,
    pub installed: bool,
    pub manifest_url: Option<String>,
    pub service_worker: Option<ServiceWorkerInfo>,
    pub install_prompt_available: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl PwaStatus {
    pub fn check(manifest: Option<&WebAppManifest>, sw: Option<&ServiceWorkerInfo>, is_secure: bool) -> Self {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let mut installable = is_secure;
        
        if !is_secure { errors.push("Site must be served over HTTPS".into()); installable = false; }
        if sw.is_none() { errors.push("No service worker registered".into()); installable = false; }
        
        if let Some(m) = manifest {
            if m.name.is_none() && m.short_name.is_none() { errors.push("Manifest missing name".into()); installable = false; }
            if m.start_url.is_none() { errors.push("Manifest missing start_url".into()); installable = false; }
            if m.icons.is_empty() { warnings.push("No icons in manifest".into()); }
            else if !m.icons.iter().any(|i| i.sizes.iter().any(|s| s.0 >= 192)) {
                warnings.push("No 192x192 or larger icon".into());
            }
        } else { errors.push("No manifest found".into()); installable = false; }
        
        Self { installable, installed: false, manifest_url: None, service_worker: sw.cloned(),
               install_prompt_available: installable, warnings, errors }
    }
}

/// Application panel
#[derive(Debug, Default)]
pub struct ApplicationPanel {
    pub manifest: Option<WebAppManifest>,
    pub service_workers: Vec<ServiceWorkerInfo>,
    pub pwa_status: Option<PwaStatus>,
}

impl ApplicationPanel {
    pub fn new() -> Self { Self::default() }
    
    pub fn set_manifest(&mut self, manifest: WebAppManifest) { self.manifest = Some(manifest); }
    
    pub fn add_service_worker(&mut self, info: ServiceWorkerInfo) { self.service_workers.push(info); }
    
    pub fn update_pwa_status(&mut self, is_secure: bool) {
        self.pwa_status = Some(PwaStatus::check(
            self.manifest.as_ref(), self.service_workers.first(), is_secure));
    }
    
    pub fn unregister_service_worker(&mut self, scope: &str) {
        self.service_workers.retain(|sw| sw.scope != scope);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_display_mode() {
        assert_eq!(DisplayMode::parse("standalone"), DisplayMode::Standalone);
        assert_eq!(DisplayMode::parse("invalid"), DisplayMode::Browser);
    }
    
    #[test]
    fn test_pwa_status() {
        let manifest = WebAppManifest { name: Some("Test".into()), start_url: Some("/".into()), ..Default::default() };
        let sw = ServiceWorkerInfo { script_url: "/sw.js".into(), scope: "/".into(), state: ServiceWorkerState::Activated,
                                     running: true, fetch_count: 0, last_update: None, version_id: "1".into() };
        let status = PwaStatus::check(Some(&manifest), Some(&sw), true);
        assert!(status.installable);
    }
}
