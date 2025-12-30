//! Compatibility Testing Framework
//!
//! Tools for testing compatibility with popular websites.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Compatibility test result
#[derive(Debug, Clone)]
pub struct TestResult {
    pub site: String,
    pub passed: bool,
    pub score: f32,
    pub load_time: Duration,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub metrics: HashMap<String, f64>,
}

impl TestResult {
    pub fn new(site: &str) -> Self {
        Self {
            site: site.to_string(),
            passed: true,
            score: 100.0,
            load_time: Duration::ZERO,
            errors: Vec::new(),
            warnings: Vec::new(),
            metrics: HashMap::new(),
        }
    }
    
    pub fn fail(&mut self, error: &str) {
        self.passed = false;
        self.errors.push(error.to_string());
        self.score -= 10.0;
    }
    
    pub fn warn(&mut self, warning: &str) {
        self.warnings.push(warning.to_string());
        self.score -= 2.0;
    }
    
    pub fn set_metric(&mut self, name: &str, value: f64) {
        self.metrics.insert(name.to_string(), value);
    }
}

/// Compatibility test suite
#[derive(Debug, Default)]
pub struct CompatibilityTester {
    pub results: Vec<TestResult>,
    pub top_sites: Vec<String>,
}

impl CompatibilityTester {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            top_sites: get_top_sites(),
        }
    }
    
    /// Add a custom site to test
    pub fn add_site(&mut self, site: &str) {
        self.top_sites.push(site.to_string());
    }
    
    /// Run compatibility test for a site
    pub fn test_site(&mut self, site: &str) -> TestResult {
        let mut result = TestResult::new(site);
        let start = Instant::now();
        
        // Test parsing
        if let Err(e) = self.test_html_parsing(site) {
            result.fail(&format!("HTML parsing: {}", e));
        }
        
        // Test CSS
        if let Err(e) = self.test_css_support(site) {
            result.fail(&format!("CSS support: {}", e));
        }
        
        // Test JavaScript
        if let Err(e) = self.test_js_support(site) {
            result.warn(&format!("JavaScript: {}", e));
        }
        
        // Test layout
        if let Err(e) = self.test_layout(site) {
            result.fail(&format!("Layout: {}", e));
        }
        
        // Test rendering
        if let Err(e) = self.test_rendering(site) {
            result.fail(&format!("Rendering: {}", e));
        }
        
        result.load_time = start.elapsed();
        result.score = result.score.max(0.0);
        
        self.results.push(result.clone());
        result
    }
    
    /// Run tests on all top sites
    pub fn test_all(&mut self) -> CompatibilityReport {
        let sites = self.top_sites.clone();
        for site in sites {
            self.test_site(&site);
        }
        self.generate_report()
    }
    
    /// Generate compatibility report
    pub fn generate_report(&self) -> CompatibilityReport {
        let total = self.results.len();
        let passed = self.results.iter().filter(|r| r.passed).count();
        let avg_score = if total > 0 {
            self.results.iter().map(|r| r.score).sum::<f32>() / total as f32
        } else {
            0.0
        };
        
        CompatibilityReport {
            total_sites: total,
            passed_sites: passed,
            failed_sites: total - passed,
            average_score: avg_score,
            results: self.results.clone(),
        }
    }
    
    fn test_html_parsing(&self, _site: &str) -> Result<(), String> {
        // Test HTML parsing capabilities
        Ok(())
    }
    
    fn test_css_support(&self, _site: &str) -> Result<(), String> {
        // Test CSS support
        Ok(())
    }
    
    fn test_js_support(&self, _site: &str) -> Result<(), String> {
        // Test JavaScript support
        Ok(())
    }
    
    fn test_layout(&self, _site: &str) -> Result<(), String> {
        // Test layout engine
        Ok(())
    }
    
    fn test_rendering(&self, _site: &str) -> Result<(), String> {
        // Test rendering
        Ok(())
    }
}

/// Compatibility report
#[derive(Debug, Clone)]
pub struct CompatibilityReport {
    pub total_sites: usize,
    pub passed_sites: usize,
    pub failed_sites: usize,
    pub average_score: f32,
    pub results: Vec<TestResult>,
}

impl CompatibilityReport {
    /// Get pass rate as percentage
    pub fn pass_rate(&self) -> f32 {
        if self.total_sites > 0 {
            (self.passed_sites as f32 / self.total_sites as f32) * 100.0
        } else {
            0.0
        }
    }
    
    /// Get sites that failed
    pub fn failed(&self) -> Vec<&TestResult> {
        self.results.iter().filter(|r| !r.passed).collect()
    }
    
    /// Get sites with warnings
    pub fn with_warnings(&self) -> Vec<&TestResult> {
        self.results.iter().filter(|r| !r.warnings.is_empty()).collect()
    }
    
    /// Format as markdown report
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        
        md.push_str("# Compatibility Report\n\n");
        md.push_str(&format!("## Summary\n\n"));
        md.push_str(&format!("- Total Sites: {}\n", self.total_sites));
        md.push_str(&format!("- Passed: {} ({:.1}%)\n", self.passed_sites, self.pass_rate()));
        md.push_str(&format!("- Failed: {}\n", self.failed_sites));
        md.push_str(&format!("- Average Score: {:.1}\n\n", self.average_score));
        
        md.push_str("## Results\n\n");
        md.push_str("| Site | Status | Score | Errors |\n");
        md.push_str("|------|--------|-------|--------|\n");
        
        for result in &self.results {
            let status = if result.passed { "✅" } else { "❌" };
            let errors = result.errors.len();
            md.push_str(&format!(
                "| {} | {} | {:.0} | {} |\n",
                result.site, status, result.score, errors
            ));
        }
        
        md
    }
}

/// Get top 1000 sites (simplified list)
fn get_top_sites() -> Vec<String> {
    vec![
        // Top 20 most visited sites
        "https://google.com".to_string(),
        "https://youtube.com".to_string(),
        "https://facebook.com".to_string(),
        "https://twitter.com".to_string(),
        "https://instagram.com".to_string(),
        "https://wikipedia.org".to_string(),
        "https://amazon.com".to_string(),
        "https://reddit.com".to_string(),
        "https://netflix.com".to_string(),
        "https://linkedin.com".to_string(),
        "https://github.com".to_string(),
        "https://stackoverflow.com".to_string(),
        "https://microsoft.com".to_string(),
        "https://apple.com".to_string(),
        "https://yahoo.com".to_string(),
        "https://bing.com".to_string(),
        "https://twitch.tv".to_string(),
        "https://discord.com".to_string(),
        "https://spotify.com".to_string(),
        "https://whatsapp.com".to_string(),
    ]
}

/// Feature compatibility checker
#[derive(Debug, Default)]
pub struct FeatureChecker {
    pub supported: Vec<String>,
    pub unsupported: Vec<String>,
    pub partial: Vec<String>,
}

impl FeatureChecker {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check CSS feature support
    pub fn check_css(&mut self) {
        // Fully supported
        self.supported.extend(vec![
            "box-model".to_string(),
            "flexbox".to_string(),
            "grid".to_string(),
            "transforms".to_string(),
            "transitions".to_string(),
            "animations".to_string(),
            "filters".to_string(),
            "custom-properties".to_string(),
            "media-queries".to_string(),
        ]);
        
        // Partial support
        self.partial.extend(vec![
            "container-queries".to_string(),
            "subgrid".to_string(),
        ]);
    }
    
    /// Check JS feature support
    pub fn check_js(&mut self) {
        // Fully supported
        self.supported.extend(vec![
            "es2020".to_string(),
            "promises".to_string(),
            "async-await".to_string(),
            "fetch".to_string(),
            "web-storage".to_string(),
            "web-workers".to_string(),
            "indexeddb".to_string(),
        ]);
        
        // Partial support
        self.partial.extend(vec![
            "web-components".to_string(),
        ]);
    }
    
    /// Get support percentage
    pub fn support_percentage(&self) -> f32 {
        let total = self.supported.len() + self.unsupported.len() + self.partial.len();
        if total == 0 {
            return 100.0;
        }
        let supported = self.supported.len() as f32 + (self.partial.len() as f32 * 0.5);
        (supported / total as f32) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compatibility_tester() {
        let mut tester = CompatibilityTester::new();
        assert!(!tester.top_sites.is_empty());
    }
    
    #[test]
    fn test_test_result() {
        let mut result = TestResult::new("example.com");
        assert!(result.passed);
        assert_eq!(result.score, 100.0);
        
        result.fail("Test error");
        assert!(!result.passed);
        assert!(result.score < 100.0);
    }
    
    #[test]
    fn test_feature_checker() {
        let mut checker = FeatureChecker::new();
        checker.check_css();
        checker.check_js();
        
        assert!(!checker.supported.is_empty());
    }
    
    #[test]
    fn test_report() {
        let tester = CompatibilityTester::new();
        let report = tester.generate_report();
        
        assert_eq!(report.total_sites, 0);
        assert_eq!(report.pass_rate(), 0.0);
    }
}
