use std::io::Cursor;
use libsync3::{BufferRsync, RsyncConfig};
use librsync::whole::{delta as whole_delta, patch as whole_patch, signature as whole_signature};

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
        for byte in modified.iter_mut().take((block_start + block_size).min(size)).skip(block_start) {
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
    let rsync = BufferRsync::new(RsyncConfig::default());

    let signatures = rsync.generate_signatures(&original[..]).unwrap();
    let delta = rsync.generate_delta(&signatures, &modified[..]).unwrap();
    let result = rsync.apply_delta(&original, &delta);

    let mut sig = Vec::new();
    let mut sig_cursor = Cursor::new(&original);
    whole_signature(&mut sig_cursor, &mut sig).unwrap();

    let mut librsync_delta = Vec::new();
    let mut new_cursor = Cursor::new(&modified);
    let mut sig_cursor2 = Cursor::new(&sig);
    whole_delta(&mut new_cursor, &mut sig_cursor2, &mut librsync_delta).unwrap();

    let mut librsync_result = Vec::new();
    let mut base_cursor = Cursor::new(&original);
    let mut delta_cursor = Cursor::new(&librsync_delta);
    whole_patch(&mut base_cursor, &mut delta_cursor, &mut librsync_result).unwrap();

    assert_eq!(result, modified, "xxhash3 rsync implementation failed");
    assert_eq!(librsync_result, modified, "librsync implementation failed");
}
