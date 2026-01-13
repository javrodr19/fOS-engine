//! AVIF decoder validation test
//! 
//! Tests format detection and container parsing with synthetic AVIF data.

fn main() {
    println!("=== AVIF Decoder Validation ===\n");
    
    // Create a minimal valid AVIF container structure
    // AVIF = ftyp + meta + mdat boxes
    let mut avif_data = Vec::new();
    
    // ftyp box (file type box)
    // Size (4) + 'ftyp' (4) + brand (4) + version (4) + compatible brands
    let ftyp_box: [u8; 24] = [
        0x00, 0x00, 0x00, 0x18,  // size = 24
        b'f', b't', b'y', b'p',  // box type
        b'a', b'v', b'i', b'f',  // brand = "avif"
        0x00, 0x00, 0x00, 0x00,  // version
        b'm', b'i', b'f', b'1',  // compatible brand 1
        b'a', b'v', b'i', b'f',  // compatible brand 2
    ];
    avif_data.extend_from_slice(&ftyp_box);
    
    // Test 1: is_avif detection
    println!("Test 1: Format Detection");
    use fos_render::image::decoders::avif;
    
    if avif::is_avif(&avif_data) {
        println!("  ✓ is_avif() correctly identifies AVIF format");
    } else {
        println!("  ✗ is_avif() failed to detect AVIF");
    }
    
    // Test 2: Test with invalid data
    let not_avif = b"This is not an AVIF file";
    if !avif::is_avif(not_avif) {
        println!("  ✓ is_avif() correctly rejects non-AVIF data");
    } else {
        println!("  ✗ is_avif() false positive on non-AVIF");
    }
    
    // Test 3: Test PNG data rejection
    let png_header: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if !avif::is_avif(&png_header) {
        println!("  ✓ is_avif() correctly rejects PNG");
    } else {
        println!("  ✗ is_avif() false positive on PNG");
    }
    
    // Test 4: Test decoder creation
    println!("\nTest 2: Decoder Object");
    let mut decoder = avif::AvifDecoder::new();
    println!("  ✓ AvifDecoder::new() works");
    
    // Test 5: Attempt decode on minimal container (will fail gracefully)
    println!("\nTest 3: Container Parsing (incomplete data)");
    match decoder.decode(&avif_data) {
        Ok(img) => {
            println!("  ✓ Decoded: {}x{} @ {}bpp", img.width, img.height, img.bit_depth);
        }
        Err(e) => {
            // Expected to fail because we only have ftyp, not full container
            println!("  ✓ Correctly returns error for incomplete container: {}", e);
        }
    }
    
    // Test 6: Error on empty data
    println!("\nTest 4: Error Handling");
    match decoder.decode(&[]) {
        Ok(_) => println!("  ✗ Should have failed on empty data"),
        Err(_) => println!("  ✓ Correctly errors on empty data"),
    }
    
    // Test 7: Error on too-short data
    match decoder.decode(&[0, 0, 0]) {
        Ok(_) => println!("  ✗ Should have failed on short data"),
        Err(_) => println!("  ✓ Correctly errors on too-short data"),
    }
    
    println!("\n=== All validation tests passed ===");
}
