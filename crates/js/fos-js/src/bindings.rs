//! DOM Bindings
//!
//! JavaScript bindings for DOM manipulation.

use crate::{JsValue, JsError};
use crate::engine_trait::JsContextApi;
use fos_dom::Document;
use std::sync::{Arc, Mutex};

/// Install document API into global object
pub fn install_document<C: JsContextApi>(ctx: &C, doc: Arc<Mutex<Document>>) -> Result<(), JsError> {
    let document = ctx.create_object()?;
    
    // document.getTitle
    let d = doc.clone();
    ctx.set_function(&document, "getTitle", move |_args| {
        let doc = d.lock().unwrap();
        Ok(JsValue::String(doc.title().to_string()))
    })?;
    
    // document.getElementById
    let d = doc.clone();
    ctx.set_function(&document, "getElementById", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Null);
        }
        
        let id = args[0].as_string().unwrap_or("");
        let doc = d.lock().unwrap();
        
        // Search for element with matching ID
        if let Some(node_id) = doc.get_element_by_id(id) {
            Ok(JsValue::Number(node_id.0 as f64))
        } else {
            Ok(JsValue::Null)
        }
    })?;
    
    // document.createElement
    let d = doc.clone();
    ctx.set_function(&document, "createElement", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Null);
        }
        
        let tag = args[0].as_string().unwrap_or("div");
        let mut doc = d.lock().unwrap();
        let node_id = doc.tree.create_element(tag);
        Ok(JsValue::Number(node_id.0 as f64))
    })?;
    
    // document.createTextNode
    let d = doc.clone();
    ctx.set_function(&document, "createTextNode", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Null);
        }
        
        let text = args[0].as_string().unwrap_or("");
        let mut doc = d.lock().unwrap();
        let node_id = doc.tree.create_text(text);
        Ok(JsValue::Number(node_id.0 as f64))
    })?;
    
    ctx.set_global("document", JsValue::Object)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub_engine::{StubEngine, StubContext};
    
    #[test]
    fn test_install_document() {
        let engine = Arc::new(StubEngine::new());
        let ctx = StubContext::new(engine);
        let doc = Arc::new(Mutex::new(Document::new("test://page")));
        
        install_document(&ctx, doc).unwrap();
    }
}
