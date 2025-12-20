//! fOS Browser - Main Entry Point

use fos_browser::Browser;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();
    
    log::info!("Starting fOS Browser...");
    
    // Parse command line for initial URL
    let initial_url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "about:blank".to_string());
    
    // Create and run browser
    let browser = Browser::new()?;
    browser.run(initial_url)?;
    
    Ok(())
}
