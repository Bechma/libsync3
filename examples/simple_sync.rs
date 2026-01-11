use libsync3::{BufferRsync, RsyncConfig};
use std::io::Cursor;

fn main() {
    let rsync = BufferRsync::new(RsyncConfig::default());

    // Original data (simulating "old" file)
    let original = b"Hello, world! This is the original content of the file.";

    // Modified data (simulating "new" file with some changes)
    let modified = b"Hello, Rust! This is the modified content of the file.";

    println!("Original: {:?}", String::from_utf8_lossy(original));
    println!("Modified: {:?}", String::from_utf8_lossy(modified));

    // Step 1: Generate signatures from the original data
    let signatures = rsync.generate_signatures(&original[..]).unwrap();
    println!("\nGenerated {} signature entries", signatures.len());

    // Step 2: Generate delta by comparing modified data against signatures
    let delta = rsync.generate_delta(&signatures, &modified[..]).unwrap();
    println!("Generated {} delta commands", delta.len());

    // Step 3: Apply delta to original data to reconstruct modified data
    let mut reconstructed = Vec::new();
    rsync.apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    // Verify the result
    assert_eq!(reconstructed, modified);
    println!("\nReconstructed: {:?}", String::from_utf8_lossy(&reconstructed));
    println!("Success! Original + Delta = Modified");
}
