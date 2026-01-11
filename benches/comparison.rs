use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use librsync::whole::{delta as whole_delta, patch as whole_patch, signature as whole_signature};
use libsync3::{BufferRsync, RsyncConfig};
use std::io::Cursor;

fn generate_test_data(size: usize) -> (Vec<u8>, Vec<u8>) {
    let mut original = Vec::with_capacity(size);

    let mut seed: u64 = 0xDEAD_BEEF;
    for _ in 0..size {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        original.push((seed >> 56) as u8);
    }

    let mut modified = original.clone();

    if size > 1000 {
        for i in (0..size).step_by(20) {
            modified[i] = modified[i].wrapping_add(1);
        }

        let block_start = size / 3;
        let block_size = size.min(500);
        for byte in modified
            .iter_mut()
            .take((block_start + block_size).min(size))
            .skip(block_start)
        {
            *byte = 0xFF;
        }

        let insert_pos = size / 2;
        let insert_data: Vec<u8> = (0u8..100).map(|i| i.wrapping_mul(7)).collect();
        modified.splice(insert_pos..insert_pos, insert_data);

        let delete_start = size * 3 / 4;
        let delete_end = (delete_start + 50).min(modified.len());
        if delete_start < modified.len() {
            modified.drain(delete_start..delete_end);
        }
    }

    (original, modified)
}

fn benchmark_signature_generation(c: &mut Criterion) {
    let sizes = vec![1_000, 10_000, 100_000, 1_000_000];
    let mut group = c.benchmark_group("signature_generation");

    for size in sizes {
        let (original, _) = generate_test_data(size);
        let rsync = BufferRsync::new(RsyncConfig::default());

        group.bench_with_input(BenchmarkId::new("xxhash3", size), &size, |b, _| {
            b.iter_batched(
                || original.clone(),
                |data| rsync.generate_signatures(&data[..]).unwrap(),
                criterion::BatchSize::LargeInput,
            );
        });

        group.bench_with_input(BenchmarkId::new("librsync", size), &size, |b, _| {
            b.iter_batched(
                || original.clone(),
                |data| {
                    let mut sig = Vec::new();
                    let mut cursor = Cursor::new(&data);
                    whole_signature(&mut cursor, &mut sig).unwrap();
                    sig
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

fn benchmark_delta_generation(c: &mut Criterion) {
    let sizes = vec![1_000, 10_000, 100_000, 1_000_000];
    let mut group = c.benchmark_group("delta_generation");

    for size in sizes {
        let (original, modified) = generate_test_data(size);
        let rsync = BufferRsync::new(RsyncConfig::default());
        let signatures = rsync.generate_signatures(&original[..]).unwrap();

        let mut sig = Vec::new();
        let mut cursor = Cursor::new(&original);
        whole_signature(&mut cursor, &mut sig).unwrap();

        group.bench_with_input(BenchmarkId::new("xxhash3", size), &size, |b, _| {
            b.iter_batched(
                || (signatures.clone(), modified.clone()),
                |(sigs, data)| rsync.generate_delta(&sigs, &data[..]).unwrap(),
                criterion::BatchSize::LargeInput,
            );
        });

        group.bench_with_input(BenchmarkId::new("librsync", size), &size, |b, _| {
            b.iter_batched(
                || (sig.clone(), modified.clone()),
                |(sig, data)| {
                    let mut delta = Vec::new();
                    let mut new_cursor = Cursor::new(&data);
                    let mut sig_cursor = Cursor::new(&sig);
                    whole_delta(&mut new_cursor, &mut sig_cursor, &mut delta).unwrap();
                    delta
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

fn benchmark_patch_application(c: &mut Criterion) {
    let sizes = vec![1_000, 10_000, 100_000, 1_000_000];
    let mut group = c.benchmark_group("patch_application");

    for size in sizes {
        let (original, modified) = generate_test_data(size);
        let rsync = BufferRsync::new(RsyncConfig::default());

        group.bench_with_input(BenchmarkId::new("xxhash3", size), &size, |b, _| {
            b.iter_batched(
                || {
                    let sigs = rsync.generate_signatures(&original[..]).unwrap();
                    let delta = rsync.generate_delta(&sigs, &modified[..]).unwrap();
                    (original.clone(), delta)
                },
                |(base, delta)| {
                    let mut result = Vec::new();
                    rsync
                        .apply_delta(Cursor::new(&base), &delta, &mut result)
                        .unwrap();
                    result
                },
                criterion::BatchSize::LargeInput,
            );
        });

        group.bench_with_input(BenchmarkId::new("librsync", size), &size, |b, _| {
            b.iter_batched(
                || {
                    let mut sig = Vec::new();
                    let mut cursor = Cursor::new(&original);
                    whole_signature(&mut cursor, &mut sig).unwrap();

                    let mut delta = Vec::new();
                    let mut new_cursor = Cursor::new(&modified);
                    let mut sig_cursor = Cursor::new(&sig);
                    whole_delta(&mut new_cursor, &mut sig_cursor, &mut delta).unwrap();
                    (original.clone(), delta)
                },
                |(base, delta)| {
                    let mut result = Vec::new();
                    let mut base_cursor = Cursor::new(&base);
                    let mut delta_cursor = Cursor::new(&delta);
                    whole_patch(&mut base_cursor, &mut delta_cursor, &mut result).unwrap();
                    result
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

fn benchmark_end_to_end(c: &mut Criterion) {
    let sizes = vec![1_000, 10_000, 100_000, 1_000_000];
    let mut group = c.benchmark_group("end_to_end");

    for size in sizes {
        let (original, modified) = generate_test_data(size);
        let rsync = BufferRsync::new(RsyncConfig::default());

        group.bench_with_input(BenchmarkId::new("xxhash3", size), &size, |b, _| {
            b.iter_batched(
                || (original.clone(), modified.clone()),
                |(base, modified)| {
                    let signatures = rsync.generate_signatures(&base[..]).unwrap();
                    let delta = rsync.generate_delta(&signatures, &modified[..]).unwrap();
                    let mut result = Vec::new();
                    rsync
                        .apply_delta(Cursor::new(&base), &delta, &mut result)
                        .unwrap();
                    result
                },
                criterion::BatchSize::LargeInput,
            );
        });

        group.bench_with_input(BenchmarkId::new("librsync", size), &size, |b, _| {
            b.iter_batched(
                || (original.clone(), modified.clone()),
                |(base, modified)| {
                    let mut sig = Vec::new();
                    let mut sig_cursor = Cursor::new(&base);
                    whole_signature(&mut sig_cursor, &mut sig).unwrap();

                    let mut delta = Vec::new();
                    let mut new_cursor = Cursor::new(&modified);
                    let mut sig_cursor2 = Cursor::new(&sig);
                    whole_delta(&mut new_cursor, &mut sig_cursor2, &mut delta).unwrap();

                    let mut result = Vec::new();
                    let mut base_cursor = Cursor::new(&base);
                    let mut delta_cursor = Cursor::new(&delta);
                    whole_patch(&mut base_cursor, &mut delta_cursor, &mut result).unwrap();
                    result
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_signature_generation,
    benchmark_delta_generation,
    benchmark_patch_application,
    benchmark_end_to_end,
);

criterion_main!(benches);
