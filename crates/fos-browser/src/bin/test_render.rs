//! Simple test to render Wikipedia

use fos_browser::renderer::PageRenderer;
use fos_browser::loader::Loader;

fn main() {
    env_logger::init();
    
    // Create loader and renderer
    let loader = Loader::new();
    let renderer = PageRenderer::new(800, 600);
    
    // Test with a simple page first
    let test_html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Test Page</title></head>
        <body>
            <h1>Hello World</h1>
            <p>This is a test of the fOS browser rendering engine.</p>
            <ul>
                <li>Item one</li>
                <li>Item two</li>
                <li>Item three</li>
            </ul>
        </body>
        </html>
    "#;
    
    println!("Rendering test HTML...");
    
    match renderer.render_html(test_html, "about:test") {
        Some(rendered) => {
            println!("✓ Rendered: {}x{} ({} bytes)", 
                rendered.width, 
                rendered.height,
                rendered.pixels.len()
            );
            println!("  Content height: {}", rendered.content_height);
        }
        None => {
            println!("✗ Failed to render");
        }
    }
    
    // Now try Wikipedia (optional, requires network)
    println!("\nAttempting to load Wikipedia...");
    match loader.load_sync("https://en.wikipedia.org/wiki/Main_Page") {
        Ok(page) => {
            println!("✓ Loaded: {} ({} bytes HTML)", 
                page.title.as_deref().unwrap_or("(no title)"),
                page.html.len()
            );
            
            // Render it
            match renderer.render_html(&page.html, &page.url) {
                Some(rendered) => {
                    println!("✓ Rendered: {}x{} ({} bytes pixels)", 
                        rendered.width, 
                        rendered.height,
                        rendered.pixels.len()
                    );
                    
                    // Save to file for inspection
                    save_png(&rendered.pixels, rendered.width, rendered.height, "wikipedia_render.png");
                    println!("✓ Saved to wikipedia_render.png");
                }
                None => {
                    println!("✗ Failed to render Wikipedia");
                }
            }
        }
        Err(e) => {
            println!("✗ Failed to load Wikipedia: {}", e);
        }
    }
}

/// Save RGBA pixels to PNG
fn save_png(pixels: &[u8], width: u32, height: u32, path: &str) {
    use std::io::Write;
    use std::fs::File;
    
    // Simple PPM format (easier than PNG dependency)
    let ppm_path = path.replace(".png", ".ppm");
    let mut file = File::create(&ppm_path).unwrap();
    
    // PPM header
    writeln!(file, "P3").unwrap();
    writeln!(file, "{} {}", width, height).unwrap();
    writeln!(file, "255").unwrap();
    
    // Pixel data (RGB only, skip alpha)
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            if i + 2 < pixels.len() {
                write!(file, "{} {} {} ", pixels[i], pixels[i+1], pixels[i+2]).unwrap();
            }
        }
        writeln!(file).unwrap();
    }
    
    println!("  (Also saved as {})", ppm_path);
}
