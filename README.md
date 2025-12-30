# libsync3

A simple, pure Rust implementation of the rsync algorithm using BLAKE3 for hashing.

This library allows you to efficiently synchronize files by calculating the differences (delta) between two versions of a file and applying those differences to update the old version.

## Features

- **BLAKE3 Hashing**: Uses the fast and secure BLAKE3 hash function.
- **Pure Rust**: No external C dependencies (other than what `blake3` might need, which is usually minimal/optional).
- **Efficient**: Optimized delta calculation with buffered I/O and minimized memory allocations.
- **Simple API**: Easy to use functions for signature generation, delta calculation, and patching.

## Usage

Here is a quick example of how to use the library:

```rust
use std::io::Cursor;
use libsync3::{signature, delta, apply, apply_to_vec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let old_data = b"Hello World";
    let new_data = b"Hello Rust World";

    // 1. Generate signature of the old data
    let sig = signature(Cursor::new(old_data))?;

    // 2. Compute delta between new data and the signature
    let diff = delta(Cursor::new(new_data), &sig)?;

    // 3. Apply delta to old data to get new data
    let result = apply_to_vec(Cursor::new(old_data), &diff)?;

    assert_eq!(result, new_data);
    
    // Optionally:
    let mut output = Vec::with_capacity(diff.final_size);
    apply(Cursor::new(old_data), &diff, &mut output)?;
    assert_eq!(output, new_data);
    
    Ok(())
}
```

## Examples

The `examples/` directory contains complete working examples:

- **simple_sync**: Demonstrates basic in-memory synchronization.
- **file_sync**: Demonstrates how to synchronize actual files on disk.

To run the examples:

```bash
# Run the simple in-memory example
cargo run --example simple_sync

# Run the file synchronization example
cargo run --example file_sync
```

## Testing

To run the test suite, including property-based tests:

```bash
cargo test --release
```

## License

Distributed under the MIT License. See [LICENSE](LICENSE) for more information.
