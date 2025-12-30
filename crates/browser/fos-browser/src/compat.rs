//! Compatibility Testing Integration
//!
//! Integrates fos-engine compatibility testing for browser verification.

use fos_engine::{
    CompatibilityTester, CompatibilityReport, TestResult, FeatureChecker,
};

/// Compatibility manager for testing browser against popular sites
pub struct CompatibilityManager {
    /// Compatibility tester
    tester: CompatibilityTester,
    /// Feature checker
    features: FeatureChecker,
}

impl CompatibilityManager {
    /// Create new compatibility manager
    pub fn new() -> Self {
        let mut features = FeatureChecker::new();
        features.check_css();
        features.check_js();
        
        Self {
            tester: CompatibilityTester::new(),
            features,
        }
    }
    
    // === Site Testing ===
    
    /// Add a site to test
    pub fn add_site(&mut self, site: &str) {
        self.tester.add_site(site);
    }
    
    /// Test a specific site
    pub fn test_site(&mut self, site: &str) -> TestResult {
        self.tester.test_site(site)
    }
    
    /// Test all configured sites
    pub fn test_all(&mut self) -> CompatibilityReport {
        self.tester.test_all()
    }
    
    /// Get the list of top sites
    pub fn top_sites(&self) -> &[String] {
        &self.tester.top_sites
    }
    
    /// Get previous test results
    pub fn results(&self) -> &[TestResult] {
        &self.tester.results
    }
    
    /// Generate report from previous results
    pub fn report(&self) -> CompatibilityReport {
        self.tester.generate_report()
    }
    
    // === Feature Checking ===
    
    /// Get supported features
    pub fn supported_features(&self) -> &[String] {
        &self.features.supported
    }
    
    /// Get partially supported features
    pub fn partial_features(&self) -> &[String] {
        &self.features.partial
    }
    
    /// Get unsupported features
    pub fn unsupported_features(&self) -> &[String] {
        &self.features.unsupported
    }
    
    /// Get feature support percentage
    pub fn feature_support_percent(&self) -> f32 {
        self.features.support_percentage()
    }
    
    /// Get compatibility summary
    pub fn summary(&self) -> CompatibilitySummary {
        let report = self.tester.generate_report();
        CompatibilitySummary {
            sites_tested: report.total_sites,
            sites_passed: report.passed_sites,
            pass_rate: report.pass_rate(),
            avg_score: report.average_score,
            feature_support: self.features.support_percentage(),
            supported_count: self.features.supported.len(),
            partial_count: self.features.partial.len(),
            unsupported_count: self.features.unsupported.len(),
        }
    }
}

impl Default for CompatibilityManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Compatibility summary
#[derive(Debug, Clone)]
pub struct CompatibilitySummary {
    pub sites_tested: usize,
    pub sites_passed: usize,
    pub pass_rate: f32,
    pub avg_score: f32,
    pub feature_support: f32,
    pub supported_count: usize,
    pub partial_count: usize,
    pub unsupported_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compatibility_creation() {
        let manager = CompatibilityManager::new();
        assert!(!manager.top_sites().is_empty());
    }
    
    #[test]
    fn test_feature_checking() {
        let manager = CompatibilityManager::new();
        assert!(!manager.supported_features().is_empty());
    }
    
    #[test]
    fn test_summary() {
        let manager = CompatibilityManager::new();
        let summary = manager.summary();
        
        assert!(summary.feature_support > 0.0);
    }
    
    #[test]
    fn test_site_testing() {
        let mut manager = CompatibilityManager::new();
        let result = manager.test_site("https://example.com");
        
        assert!(result.passed);
    }
}
