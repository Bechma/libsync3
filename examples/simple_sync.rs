use std::io::Cursor;
use libsync3::{signature, delta, apply_to_vec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup initial data (old version) and modified data (new version)
    let old_data = b"Hello, world! This is the original version of the file.";
    let new_data = b"Hello, Rust! This is the modified version of the file.";

    println!("Original: {:?}", String::from_utf8_lossy(old_data));
    println!("Modified: {:?}", String::from_utf8_lossy(new_data));

    // 2. Generate signature for the old data
    // In a real networked scenario, the receiver (who has old_data) would send this signature to the sender.
    let sig = signature(Cursor::new(old_data))?;
    println!("Generated signature with {} chunks", sig.chunks.len());

    // 3. Compute delta
    // The sender (who has new_data) uses the signature to compute the difference.
    let d = delta(Cursor::new(new_data), &sig)?;
    println!("Computed delta with {} operations", d.ops.len());

    // 4. Apply delta
    // The receiver applies the delta to their old_data to reconstruct new_data.
    let reconstructed = apply_to_vec(Cursor::new(old_data), &d)?;
    
    println!("Reconstructed: {:?}", String::from_utf8_lossy(&reconstructed));

    // Verify
    assert_eq!(reconstructed, new_data);
    println!("Success! Reconstructed data matches modified data.");

    Ok(())
}
