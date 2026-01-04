//! Sources Panel
//!
//! Source file viewing, source maps, pretty printing, and file search.

use std::collections::HashMap;

/// Source file
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub url: String,
    pub content: String,
    pub content_type: SourceType,
    pub source_map_url: Option<String>,
    pub pretty_printed: bool,
}

/// Source type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType { JavaScript, TypeScript, Css, Html, Json, Wasm, Other }

impl SourceType {
    pub fn from_mime(mime: &str) -> Self {
        match mime {
            "application/javascript" | "text/javascript" => Self::JavaScript,
            "application/typescript" => Self::TypeScript,
            "text/css" => Self::Css,
            "text/html" => Self::Html,
            "application/json" => Self::Json,
            "application/wasm" => Self::Wasm,
            _ => Self::Other,
        }
    }
}

/// Source map
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    pub version: u8,
    pub file: Option<String>,
    pub sources: Vec<String>,
    pub sources_content: Vec<Option<String>>,
    pub names: Vec<String>,
    pub mappings: String,
}

impl SourceMap {
    /// Parse source map from JSON
    pub fn parse(json: &str) -> Option<Self> {
        // Simplified parsing - real impl would use serde
        let mut map = Self::default();
        map.version = 3;
        
        // Extract sources array
        if let Some(start) = json.find("\"sources\"") {
            if let Some(arr_start) = json[start..].find('[') {
                if let Some(arr_end) = json[start + arr_start..].find(']') {
                    let arr = &json[start + arr_start + 1..start + arr_start + arr_end];
                    map.sources = arr.split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
        }
        
        Some(map)
    }
    
    /// Map generated position to original
    pub fn original_position(&self, _line: u32, _column: u32) -> Option<OriginalPosition> {
        // VLQ decoding would go here
        None
    }
}

/// Original source position
#[derive(Debug, Clone)]
pub struct OriginalPosition {
    pub source: String,
    pub line: u32,
    pub column: u32,
    pub name: Option<String>,
}

/// Pretty printer for JavaScript
#[derive(Debug)]
pub struct JsPrettyPrinter {
    indent_size: usize,
}

impl JsPrettyPrinter {
    pub fn new(indent_size: usize) -> Self { Self { indent_size } }
    
    pub fn format(&self, code: &str) -> String {
        let mut output = String::new();
        let mut indent = 0;
        let mut in_string = false;
        let mut string_char = ' ';
        
        for c in code.chars() {
            if in_string {
                output.push(c);
                if c == string_char { in_string = false; }
                continue;
            }
            
            match c {
                '"' | '\'' | '`' => { in_string = true; string_char = c; output.push(c); }
                '{' => { output.push(c); indent += 1; output.push('\n'); output.push_str(&" ".repeat(indent * self.indent_size)); }
                '}' => { indent = indent.saturating_sub(1); output.push('\n'); output.push_str(&" ".repeat(indent * self.indent_size)); output.push(c); }
                ';' => { output.push(c); output.push('\n'); output.push_str(&" ".repeat(indent * self.indent_size)); }
                _ => output.push(c),
            }
        }
        output
    }
}

/// File search
#[derive(Debug)]
pub struct FileSearch {
    files: HashMap<String, SourceFile>,
}

impl FileSearch {
    pub fn new() -> Self { Self { files: HashMap::new() } }
    
    pub fn add_file(&mut self, file: SourceFile) { self.files.insert(file.url.clone(), file); }
    
    pub fn search_by_name(&self, query: &str) -> Vec<&SourceFile> {
        let query = query.to_lowercase();
        self.files.values().filter(|f| f.url.to_lowercase().contains(&query)).collect()
    }
    
    pub fn search_in_content(&self, query: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        for file in self.files.values() {
            for (i, line) in file.content.lines().enumerate() {
                if line.contains(query) {
                    results.push(SearchResult { url: file.url.clone(), line: i + 1, content: line.to_string() });
                }
            }
        }
        results
    }
}

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub url: String,
    pub line: usize,
    pub content: String,
}

/// Sources panel
#[derive(Debug, Default)]
pub struct SourcesPanel {
    files: HashMap<String, SourceFile>,
    source_maps: HashMap<String, SourceMap>,
    search: Option<FileSearch>,
    open_file: Option<String>,
}

impl SourcesPanel {
    pub fn new() -> Self { Self::default() }
    
    pub fn add_source(&mut self, file: SourceFile) { self.files.insert(file.url.clone(), file); }
    pub fn add_source_map(&mut self, url: &str, map: SourceMap) { self.source_maps.insert(url.into(), map); }
    pub fn open_file(&mut self, url: &str) { self.open_file = Some(url.into()); }
    pub fn get_file(&self, url: &str) -> Option<&SourceFile> { self.files.get(url) }
    pub fn get_all_files(&self) -> Vec<&str> { self.files.keys().map(|s| s.as_str()).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pretty_print() {
        let printer = JsPrettyPrinter::new(2);
        let code = "function f(){return 1;}";
        let formatted = printer.format(code);
        assert!(formatted.contains('\n'));
    }
    
    #[test]
    fn test_file_search() {
        let mut search = FileSearch::new();
        search.add_file(SourceFile { url: "/app.js".into(), content: "const x = 1;".into(),
            content_type: SourceType::JavaScript, source_map_url: None, pretty_printed: false });
        
        assert_eq!(search.search_by_name("app").len(), 1);
        assert_eq!(search.search_in_content("const").len(), 1);
    }
}
