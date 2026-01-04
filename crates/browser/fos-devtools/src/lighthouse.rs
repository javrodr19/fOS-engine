//! Lighthouse/Audit Panel
//!
//! Performance scoring, accessibility audits, best practices, and SEO.

/// Audit category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditCategory { Performance, Accessibility, BestPractices, Seo, Pwa }

/// Audit result
#[derive(Debug, Clone)]
pub struct AuditResult {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: AuditCategory,
    pub score: Option<f64>, // 0.0 to 1.0
    pub score_display: ScoreDisplay,
    pub details: Option<AuditDetails>,
}

/// Score display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreDisplay { Numeric, Binary, Informative, NotApplicable, Manual }

/// Audit details
#[derive(Debug, Clone)]
pub enum AuditDetails {
    Table { headings: Vec<String>, rows: Vec<Vec<String>> },
    Opportunity { overallSavingsMs: f64, items: Vec<OpportunityItem> },
    Diagnostic { items: Vec<DiagnosticItem> },
    TreeMap { nodes: Vec<TreeMapNode> },
}

/// Opportunity item
#[derive(Debug, Clone)]
pub struct OpportunityItem {
    pub url: String,
    pub wastedMs: f64,
    pub totalBytes: Option<usize>,
}

/// Diagnostic item
#[derive(Debug, Clone)]
pub struct DiagnosticItem {
    pub label: String,
    pub value: String,
}

/// Tree map node
#[derive(Debug, Clone)]
pub struct TreeMapNode {
    pub name: String,
    pub size: usize,
}

/// Category score
#[derive(Debug, Clone)]
pub struct CategoryScore {
    pub category: AuditCategory,
    pub score: f64,
    pub audits: Vec<AuditResult>,
}

impl CategoryScore {
    pub fn grade(&self) -> &'static str {
        if self.score >= 0.9 { "A" }
        else if self.score >= 0.5 { "B" }
        else { "C" }
    }
    
    pub fn color(&self) -> &'static str {
        if self.score >= 0.9 { "#0cce6b" }
        else if self.score >= 0.5 { "#ffa400" }
        else { "#ff4e42" }
    }
}

/// Lighthouse report
#[derive(Debug, Clone, Default)]
pub struct LighthouseReport {
    pub url: String,
    pub fetch_time: String,
    pub categories: Vec<CategoryScore>,
    pub runtime_error: Option<String>,
}

impl LighthouseReport {
    pub fn get_category(&self, cat: AuditCategory) -> Option<&CategoryScore> {
        self.categories.iter().find(|c| c.category == cat)
    }
    
    pub fn overall_score(&self) -> f64 {
        if self.categories.is_empty() { return 0.0; }
        self.categories.iter().map(|c| c.score).sum::<f64>() / self.categories.len() as f64
    }
}

/// Audit runner
#[derive(Debug, Default)]
pub struct AuditRunner {
    audits: Vec<Box<dyn Audit>>,
}

/// Audit trait
pub trait Audit: std::fmt::Debug + Send + Sync {
    fn id(&self) -> &str;
    fn title(&self) -> &str;
    fn category(&self) -> AuditCategory;
    fn run(&self, context: &AuditContext) -> AuditResult;
}

/// Audit context
#[derive(Debug, Default)]
pub struct AuditContext {
    pub url: String,
    pub html: String,
    pub load_time_ms: f64,
    pub dom_elements: usize,
    pub requests: Vec<ResourceInfo>,
    pub scripts: Vec<ScriptInfo>,
}

/// Resource info
#[derive(Debug, Clone)]
pub struct ResourceInfo {
    pub url: String,
    pub size: usize,
    pub load_time: f64,
    pub resource_type: String,
}

/// Script info
#[derive(Debug, Clone)]
pub struct ScriptInfo {
    pub url: String,
    pub size: usize,
    pub execution_time: f64,
    pub blocking: bool,
}

// Built-in audits

/// First Contentful Paint audit
#[derive(Debug)]
pub struct FcpAudit;

impl Audit for FcpAudit {
    fn id(&self) -> &str { "first-contentful-paint" }
    fn title(&self) -> &str { "First Contentful Paint" }
    fn category(&self) -> AuditCategory { AuditCategory::Performance }
    fn run(&self, context: &AuditContext) -> AuditResult {
        let score = if context.load_time_ms < 1800.0 { 1.0 }
            else if context.load_time_ms < 3000.0 { 0.5 } else { 0.0 };
        AuditResult { id: self.id().into(), title: self.title().into(),
            description: format!("FCP: {:.0}ms", context.load_time_ms),
            category: self.category(), score: Some(score), score_display: ScoreDisplay::Numeric, details: None }
    }
}

/// DOM size audit
#[derive(Debug)]
pub struct DomSizeAudit;

impl Audit for DomSizeAudit {
    fn id(&self) -> &str { "dom-size" }
    fn title(&self) -> &str { "Avoids an excessive DOM size" }
    fn category(&self) -> AuditCategory { AuditCategory::Performance }
    fn run(&self, context: &AuditContext) -> AuditResult {
        let score = if context.dom_elements < 1500 { 1.0 }
            else if context.dom_elements < 3000 { 0.5 } else { 0.0 };
        AuditResult { id: self.id().into(), title: self.title().into(),
            description: format!("{} elements", context.dom_elements),
            category: self.category(), score: Some(score), score_display: ScoreDisplay::Numeric, details: None }
    }
}

/// Lighthouse panel
#[derive(Debug, Default)]
pub struct LighthousePanel {
    last_report: Option<LighthouseReport>,
    running: bool,
}

impl LighthousePanel {
    pub fn new() -> Self { Self::default() }
    pub fn is_running(&self) -> bool { self.running }
    pub fn get_report(&self) -> Option<&LighthouseReport> { self.last_report.as_ref() }
    
    pub fn run_audit(&mut self, context: AuditContext) {
        self.running = true;
        let fcp = FcpAudit.run(&context);
        let dom = DomSizeAudit.run(&context);
        
        let perf = CategoryScore { category: AuditCategory::Performance,
            score: (fcp.score.unwrap_or(0.0) + dom.score.unwrap_or(0.0)) / 2.0, audits: vec![fcp, dom] };
        
        self.last_report = Some(LighthouseReport { url: context.url, fetch_time: "".into(),
            categories: vec![perf], runtime_error: None });
        self.running = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_category_score() {
        let score = CategoryScore { category: AuditCategory::Performance, score: 0.95, audits: vec![] };
        assert_eq!(score.grade(), "A");
        assert_eq!(score.color(), "#0cce6b");
    }
    
    #[test]
    fn test_audit_run() {
        let mut panel = LighthousePanel::new();
        panel.run_audit(AuditContext { load_time_ms: 1500.0, dom_elements: 1000, ..Default::default() });
        assert!(panel.get_report().is_some());
    }
}
