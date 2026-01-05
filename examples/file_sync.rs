use libsync3::{apply, delta, signature};
use std::fs::{self, File};
use std::io::{self, Seek, SeekFrom};

fn main() -> io::Result<()> {
    let old_path = "old_file.txt";
    let new_path = "new_file.txt";
    let patched_path = "reconstructed_file.txt";

    // Clean up previous run
    let _ = fs::remove_file(old_path);
    let _ = fs::remove_file(new_path);
    let _ = fs::remove_file(patched_path);

    // 1. Create dummy files
    println!("Creating test files...");
    fs::write(
        old_path,
        b"This is a large-ish file.\nIt has multiple lines.\nSome stay the same.\n",
    )?;
    fs::write(new_path, b"This is a large-ish file.\nIt has CHANGED lines.\nSome stay the same.\nAnd new lines added.\n")?;

    println!("Old file size: {} bytes", fs::metadata(old_path)?.len());
    println!("New file size: {} bytes", fs::metadata(new_path)?.len());

    // 2. Generate signature of the old file
    println!("Generating signature of {old_path}");
    let mut old_file = File::open(old_path)?;
    let sig = signature(&mut old_file)?;

    // 3. Calculate delta between new file and old signature
    println!("Calculating delta for {new_path}");
    let mut new_file = File::open(new_path)?;
    let diff = delta(&mut new_file, &sig)?;

    println!("Delta contains {} operations", diff.ops.len());

    // 4. Apply delta to old file to create the patched file
    println!("Applying delta to reconstruct new content at {patched_path}");
    // Note: 'apply' needs the original file to be seekable to read matching chunks
    old_file.seek(SeekFrom::Start(0))?;
    let mut patched_file = File::create(patched_path)?;
    apply(old_file, &diff, &mut patched_file)?;

    // Verify
    let new_content = fs::read(new_path)?;
    let patched_content = fs::read(patched_path)?;

    if new_content == patched_content {
        println!("Success! {patched_path} matches {new_path}");
    } else {
        eprintln!("Error! Files do not match.");
        std::process::exit(1);
    }

    // Cleanup
    fs::remove_file(old_path)?;
    fs::remove_file(new_path)?;
    fs::remove_file(patched_path)?;

    Ok(())
}
