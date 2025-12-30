//! Form submission integration
//!
//! Connects fos-dom/forms to browser navigation for form POST/GET.

use std::collections::HashMap;
use fos_dom::{Document, DomTree, NodeId};

/// Form submission method
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormMethod {
    Get,
    Post,
}

impl Default for FormMethod {
    fn default() -> Self {
        Self::Get
    }
}

/// Form encoding type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormEnctype {
    UrlEncoded,
    Multipart,
    TextPlain,
}

impl Default for FormEnctype {
    fn default() -> Self {
        Self::UrlEncoded
    }
}

/// Collected form data
#[derive(Debug, Clone)]
pub struct FormData {
    pub action: String,
    pub method: FormMethod,
    pub enctype: FormEnctype,
    pub fields: HashMap<String, FormValue>,
}

/// Form field value
#[derive(Debug, Clone)]
pub enum FormValue {
    Text(String),
    File { name: String, content: Vec<u8>, mime_type: String },
    Multiple(Vec<String>),
}

impl FormData {
    pub fn new(action: &str) -> Self {
        Self {
            action: action.to_string(),
            method: FormMethod::Get,
            enctype: FormEnctype::UrlEncoded,
            fields: HashMap::new(),
        }
    }
    
    /// Add a text field
    pub fn add_field(&mut self, name: &str, value: &str) {
        self.fields.insert(name.to_string(), FormValue::Text(value.to_string()));
    }
    
    /// Encode as URL query string
    pub fn to_query_string(&self) -> String {
        let mut parts = Vec::new();
        for (name, value) in &self.fields {
            if let FormValue::Text(v) = value {
                let encoded_name = urlencoding::encode(name);
                let encoded_value = urlencoding::encode(v);
                parts.push(format!("{}={}", encoded_name, encoded_value));
            }
        }
        parts.join("&")
    }
    
    /// Encode as multipart/form-data body
    pub fn to_multipart(&self, boundary: &str) -> Vec<u8> {
        let mut body = Vec::new();
        
        for (name, value) in &self.fields {
            body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
            
            match value {
                FormValue::Text(v) => {
                    body.extend_from_slice(
                        format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n", name, v).as_bytes()
                    );
                }
                FormValue::File { name: filename, content, mime_type } => {
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n\
                             Content-Type: {}\r\n\r\n",
                            name, filename, mime_type
                        ).as_bytes()
                    );
                    body.extend_from_slice(content);
                    body.extend_from_slice(b"\r\n");
                }
                FormValue::Multiple(values) => {
                    for v in values {
                        body.extend_from_slice(
                            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n", name, v).as_bytes()
                        );
                    }
                }
            }
        }
        
        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
        body
    }
    
    /// Build final URL for GET submission
    pub fn build_get_url(&self, base_url: &str) -> String {
        let query = self.to_query_string();
        if query.is_empty() {
            self.action.clone()
        } else if self.action.contains('?') {
            format!("{}&{}", self.action, query)
        } else {
            format!("{}?{}", self.action, query)
        }
    }
}

/// Form collector - extracts form data from DOM
pub struct FormCollector;

impl FormCollector {
    /// Collect form data from a form element
    pub fn collect(tree: &DomTree, form_id: NodeId) -> Option<FormData> {
        let form_node = tree.get(form_id)?;
        let form_elem = form_node.as_element()?;
        
        // Get form attributes
        let mut action = String::from("");
        let mut method = FormMethod::Get;
        let mut enctype = FormEnctype::UrlEncoded;
        
        for attr in form_elem.attrs.iter() {
            let name = tree.resolve(attr.name.local);
            match name {
                "action" => action = attr.value.to_string(),
                "method" => {
                    method = match attr.value.to_lowercase().as_str() {
                        "post" => FormMethod::Post,
                        _ => FormMethod::Get,
                    };
                }
                "enctype" => {
                    enctype = match attr.value.as_ref() {
                        "multipart/form-data" => FormEnctype::Multipart,
                        "text/plain" => FormEnctype::TextPlain,
                        _ => FormEnctype::UrlEncoded,
                    };
                }
                _ => {}
            }
        }
        
        let mut form_data = FormData::new(&action);
        form_data.method = method;
        form_data.enctype = enctype;
        
        // Collect input fields
        Self::collect_inputs(tree, form_id, &mut form_data);
        
        Some(form_data)
    }
    
    /// Recursively collect inputs from form
    fn collect_inputs(tree: &DomTree, node_id: NodeId, form_data: &mut FormData) {
        for (child_id, child_node) in tree.children(node_id) {
            if let Some(elem) = child_node.as_element() {
                let tag = tree.resolve(elem.name.local).to_lowercase();
                
                match tag.as_str() {
                    "input" => Self::collect_input(tree, child_id, elem, form_data),
                    "select" => Self::collect_select(tree, child_id, elem, form_data),
                    "textarea" => Self::collect_textarea(tree, child_id, elem, form_data),
                    _ => {}
                }
            }
            
            // Recurse
            Self::collect_inputs(tree, child_id, form_data);
        }
    }
    
    fn collect_input(tree: &DomTree, _node_id: NodeId, elem: &fos_dom::ElementData, form_data: &mut FormData) {
        let mut name = None;
        let mut value = String::new();
        let mut input_type = "text";
        let mut checked = false;
        let mut disabled = false;
        
        for attr in elem.attrs.iter() {
            let attr_name = tree.resolve(attr.name.local);
            match attr_name {
                "name" => name = Some(attr.value.to_string()),
                "value" => value = attr.value.to_string(),
                "type" => input_type = Box::leak(attr.value.to_string().into_boxed_str()),
                "checked" => checked = true,
                "disabled" => disabled = true,
                _ => {}
            }
        }
        
        if disabled {
            return;
        }
        
        if let Some(name) = name {
            match input_type {
                "checkbox" | "radio" => {
                    if checked {
                        form_data.add_field(&name, if value.is_empty() { "on" } else { &value });
                    }
                }
                "submit" | "image" | "button" | "reset" => {
                    // Skip submit buttons (handled separately)
                }
                "file" => {
                    // File inputs need special handling
                }
                _ => {
                    form_data.add_field(&name, &value);
                }
            }
        }
    }
    
    fn collect_select(tree: &DomTree, node_id: NodeId, elem: &fos_dom::ElementData, form_data: &mut FormData) {
        let mut name = None;
        let mut disabled = false;
        
        for attr in elem.attrs.iter() {
            let attr_name = tree.resolve(attr.name.local);
            match attr_name {
                "name" => name = Some(attr.value.to_string()),
                "disabled" => disabled = true,
                _ => {}
            }
        }
        
        if disabled {
            return;
        }
        
        if let Some(name) = name {
            // Find selected option(s)
            for (opt_id, opt_node) in tree.children(node_id) {
                if let Some(opt_elem) = opt_node.as_element() {
                    let tag = tree.resolve(opt_elem.name.local);
                    if tag.eq_ignore_ascii_case("option") {
                        let mut selected = false;
                        let mut value = String::new();
                        
                        for attr in opt_elem.attrs.iter() {
                            let attr_name = tree.resolve(attr.name.local);
                            match attr_name {
                                "selected" => selected = true,
                                "value" => value = attr.value.to_string(),
                                _ => {}
                            }
                        }
                        
                        if selected {
                            // If no value attribute, use text content
                            if value.is_empty() {
                                for (_, text_node) in tree.children(opt_id) {
                                    if let Some(text) = text_node.as_text() {
                                        value = text.trim().to_string();
                                        break;
                                    }
                                }
                            }
                            form_data.add_field(&name, &value);
                        }
                    }
                }
            }
        }
    }
    
    fn collect_textarea(tree: &DomTree, node_id: NodeId, elem: &fos_dom::ElementData, form_data: &mut FormData) {
        let mut name = None;
        let mut disabled = false;
        
        for attr in elem.attrs.iter() {
            let attr_name = tree.resolve(attr.name.local);
            match attr_name {
                "name" => name = Some(attr.value.to_string()),
                "disabled" => disabled = true,
                _ => {}
            }
        }
        
        if disabled {
            return;
        }
        
        if let Some(name) = name {
            // Get text content
            let mut value = String::new();
            for (_, text_node) in tree.children(node_id) {
                if let Some(text) = text_node.as_text() {
                    value.push_str(text);
                }
            }
            form_data.add_field(&name, &value);
        }
    }
}

// URL encoding helper
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                    result.push(c);
                }
                ' ' => result.push('+'),
                _ => {
                    for b in c.encode_utf8(&mut [0; 4]).bytes() {
                        result.push_str(&format!("%{:02X}", b));
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_form_data_query_string() {
        let mut form = FormData::new("/search");
        form.add_field("q", "rust programming");
        form.add_field("page", "1");
        
        let query = form.to_query_string();
        assert!(query.contains("q=rust+programming"));
        assert!(query.contains("page=1"));
    }
    
    #[test] 
    fn test_url_encoding() {
        assert_eq!(urlencoding::encode("hello world"), "hello+world");
        assert_eq!(urlencoding::encode("a=b&c=d"), "a%3Db%26c%3Dd");
    }
}
