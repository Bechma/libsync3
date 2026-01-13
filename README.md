# libsync3

A simple, pure Rust implementation of the rsync algorithm using xxhash3 for hashing.

This library allows you to efficiently calculate the differences (delta) between two versions of
a file or array of bytes and applying those differences to update the old version.

- **Pure Rust**: No external C dependencies.
- **Portable SIMD**: Uses Adler32 with portable SIMD for fast rolling checksums.
- **xxhash3 Hashing**: Uses the xxhash3 128 bits hash function for strong checksums.

## Usage

Here is a quick example of how to use the library:

```rust
use libsync3::{apply_delta, generate_delta, generate_signatures};

fn main() {
    let original = b"Hello, world! This is the original content.";
    let modified = b"Hello, Rust! This is the modified content.";

    // 1. Generate signatures from the original data
    let signatures = generate_signatures(&original[..]).unwrap();

    // 2. Generate delta by comparing modified data against signatures
    let delta = generate_delta(&signatures, &modified[..]).unwrap();

    // 3. Apply delta to original data to reconstruct modified data
    let mut reconstructed = Vec::new();
    apply_delta(std::io::Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}
```

## Benchmarks

Performance comparison between libsync3 (xxhash3) and librsync (end-to-end: delta generation + patch application):

Done in an AMD Ryzen 9 7900X.

```bash
cargo bench
```

## Examples

The `examples/` directory contains complete working examples:

- **simple_sync**: Demonstrates basic in-memory synchronization.

To run the examples:

```bash
cargo run --example simple_sync
```

## Testing

To run the test suite:

```bash
cargo test
```

## License

Distributed under the MIT License. See [LICENSE](LICENSE) for more information.
