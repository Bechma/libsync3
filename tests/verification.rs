use librsync::whole::{delta as whole_delta, patch as whole_patch, signature as whole_signature};
use libsync3::{apply_delta, generate_delta, generate_signatures};
use std::io::Cursor;

fn libsync3_roundtrip(original: &[u8], modified: &[u8]) -> Vec<u8> {
    let signatures = generate_signatures(original).unwrap();
    let delta = generate_delta(&signatures, modified).unwrap();

    let mut result = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut result).unwrap();
    result
}

fn librsync_roundtrip(original: &[u8], modified: &[u8]) -> Vec<u8> {
    let mut sig = Vec::new();
    whole_signature(&mut Cursor::new(original), &mut sig).unwrap();

    let mut librsync_delta = Vec::new();
    whole_delta(
        &mut Cursor::new(modified),
        &mut Cursor::new(&sig),
        &mut librsync_delta,
    )
    .unwrap();

    let mut librsync_result = Vec::new();
    whole_patch(
        &mut Cursor::new(original),
        &mut Cursor::new(&librsync_delta),
        &mut librsync_result,
    )
    .unwrap();

    librsync_result
}

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

#[test]
fn verify_correctness() {
    let (original, modified) = generate_test_data(50_000);

    let result = libsync3_roundtrip(&original, &modified);
    let librsync_result = librsync_roundtrip(&original, &modified);

    assert_eq!(result, modified, "xxhash3 rsync implementation failed");
    assert_eq!(librsync_result, modified, "librsync implementation failed");
}
