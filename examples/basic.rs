//! Example: Basic usage of fOS Engine

use fos_engine::{Engine, Config};

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Create engine
    let config = Config::default();
    let engine = Engine::new(config);
    
    println!("fOS Engine v{} initialized", fos_engine::VERSION);
    println!("Ready to load pages!");
    
    // TODO: Add async runtime and load a page
    // smol::block_on(async {
    //     let page = engine.load_url("https://example.com").await.unwrap();
    //     let canvas = page.render(800, 600);
    //     println!("Rendered {}x{} pixels", canvas.width, canvas.height);
    // });
}
