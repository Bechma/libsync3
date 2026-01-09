# libsync3

A simple, pure Rust implementation of the rsync algorithm using xxhash3 for hashing.

This library allows you to efficiently synchronize files by calculating the differences (delta) between two versions of
a file and applying those differences to update the old version.

## Features

- **xxhash3 Hashing**: Uses the extremely fast xxhash3 hash function for both rolling and strong checksums.
- **Pure Rust**: No external C dependencies.
- **Parallel Processing**: Uses rayon for parallel signature generation.
- **Simple API**: Easy to use `BufferRsync` struct for signature generation, delta calculation, and patching.

## Usage

Here is a quick example of how to use the library:

```rust
use libsync3::{BufferRsync, RsyncConfig};

fn main() {
    let rsync = BufferRsync::new(RsyncConfig::default());

    let original = b"Hello, world! This is the original content.";
    let modified = b"Hello, Rust! This is the modified content.";

    // 1. Generate signatures from the original data
    let signatures = rsync.generate_signatures(&original[..]).unwrap();

    // 2. Generate delta by comparing modified data against signatures
    let delta = rsync.generate_delta(&signatures, &modified[..]).unwrap();

    // 3. Apply delta to original data to reconstruct modified data
    let reconstructed = rsync.apply_delta(original, &delta);

    assert_eq!(reconstructed, modified);
}
```

## Benchmarks

Performance comparison between libsync3 (xxhash3) and librsync (end-to-end: delta generation + patch application):

```bash
cargo bench
```

| Data Size | libsync3 (xxhash3) | librsync | Speedup |
|-----------|--------------------|----------|---------|
| 1 KB      | 113 ns             | 1.78 µs  | ~16x    |
| 5 KB      | 722 ns             | 26.1 µs  | ~36x    |
| 10 KB     | 1.21 µs            | 49.4 µs  | ~41x    |
| 50 KB     | 5.00 µs            | 618 µs   | ~124x   |
| 100 KB    | 10.8 µs            | 1.25 ms  | ~116x   |
| 1 MB      | 648 µs             | 11.4 ms  | ~18x    |

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
