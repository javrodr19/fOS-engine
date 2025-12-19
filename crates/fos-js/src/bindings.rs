//! DOM Bindings for JavaScript
//!
//! Bridges JavaScript to the fos-dom Document.

use rquickjs::{Ctx, Function, Object, Value};
use fos_dom::{Document, NodeId, DomTree};
use std::sync::{Arc, Mutex};

/// Install document object into global
pub fn install_document(ctx: &Ctx, doc: Arc<Mutex<Document>>) -> Result<(), rquickjs::Error> {
    let globals = ctx.globals();
    
    let document = Object::new(ctx.clone())?;
    
    // document.title (getter)
    let doc_ref = doc.clone();
    document.set("getTitle", Function::new(ctx.clone(), move |_ctx: Ctx| -> Result<String, rquickjs::Error> {
        let doc = doc_ref.lock().unwrap();
        Ok(doc.title())
    })?)?;
    
    // document.getElementById (returns node ID as number, or -1 if not found)
    let doc_ref = doc.clone();
    document.set("getElementById", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<i32, rquickjs::Error> {
        if let Some(id_str) = args.first().and_then(|v| v.as_string()) {
            let id = id_str.to_string().unwrap_or_default();
            let doc = doc_ref.lock().unwrap();
            
            // Search for element with matching id
            if let Some(node_id) = find_element_by_id(doc.tree(), &id) {
                return Ok(node_id.0 as i32);
            }
        }
        Ok(-1)
    })?)?;
    
    // document.createElement
    let doc_ref = doc.clone();
    document.set("createElement", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<i32, rquickjs::Error> {
        if let Some(tag_str) = args.first().and_then(|v| v.as_string()) {
            let tag = tag_str.to_string().unwrap_or_default();
            let mut doc = doc_ref.lock().unwrap();
            let id = doc.tree_mut().create_element(&tag);
            return Ok(id.0 as i32);
        }
        Ok(-1)
    })?)?;
    
    // document.createTextNode
    let doc_ref = doc.clone();
    document.set("createTextNode", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<i32, rquickjs::Error> {
        if let Some(text_str) = args.first().and_then(|v| v.as_string()) {
            let text = text_str.to_string().unwrap_or_default();
            let mut doc = doc_ref.lock().unwrap();
            let id = doc.tree_mut().create_text(&text);
            return Ok(id.0 as i32);
        }
        Ok(-1)
    })?)?;
    
    globals.set("document", document)?;
    
    Ok(())
}

/// Find an element by its id attribute
fn find_element_by_id(tree: &DomTree, id: &str) -> Option<NodeId> {
    // Linear search through all nodes
    for i in 0..tree.len() {
        let node_id = NodeId(i as u32);
        if let Some(node) = tree.get(node_id) {
            if let Some(elem) = node.as_element() {
                if let Some(elem_id) = elem.id {
                    if tree.resolve(elem_id) == id {
                        return Some(node_id);
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rquickjs::{Runtime, Context};
    
    #[test]
    fn test_document_object() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        let doc = Arc::new(Mutex::new(Document::new("test://page")));
        
        context.with(|ctx| {
            install_document(&ctx, doc).unwrap();
            
            // Check document exists
            let result: Value = ctx.eval("typeof document").unwrap();
            assert_eq!(result.as_string().unwrap().to_string().unwrap(), "object");
        });
    }
    
    #[test]
    fn test_create_element() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        let doc = Arc::new(Mutex::new(Document::new("test://page")));
        
        context.with(|ctx| {
            install_document(&ctx, doc.clone()).unwrap();
            
            let result: Value = ctx.eval("document.createElement('div')").unwrap();
            // Should return a valid node ID (>= 0)
            assert!(result.as_int().unwrap() >= 0);
        });
    }
    
    #[test]
    fn test_get_title() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        let doc = Arc::new(Mutex::new(Document::new("test://page")));
        
        context.with(|ctx| {
            install_document(&ctx, doc).unwrap();
            
            // Title will be empty for new document
            let result: Value = ctx.eval("document.getTitle()").unwrap();
            assert!(result.as_string().is_some());
        });
    }
}
