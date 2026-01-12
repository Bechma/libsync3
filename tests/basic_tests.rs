use libsync3::{
    DeltaCommand, apply_delta, generate_delta, generate_delta_with_block_size, generate_signatures,
    generate_signatures_with_block_size,
};
use std::io::Cursor;

#[test]
fn test_basic_rsync() {
    let original = b"Hello, world! This is a test file for rsync.";
    let modified = b"Hello, world! This is a modified test file for rsync.";

    let signatures = generate_signatures(&original[..]).unwrap();
    let delta = generate_delta(&signatures, &modified[..]).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_handles_insertions() {
    let original = b"ABCDEFGHabcdefgh";
    let modified = b"ABCXYZDEFGHabcdefgh";

    let signatures = generate_signatures(&original[..]).unwrap();
    let delta = generate_delta(&signatures, &modified[..]).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_unchanged_data() {
    let data = b"Hello, world! This is a test file for rsync.";

    let signatures = generate_signatures(&data[..]).unwrap();
    let delta = generate_delta(&signatures, &data[..]).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(data), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, data);
}

#[test]
fn test_completely_different_data() {
    let original = b"Hello, world!";
    let modified = b"Goodbye, world!";

    let signatures = generate_signatures(&original[..]).unwrap();
    let delta = generate_delta(&signatures, &modified[..]).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_1mb_with_prepended_byte_rolling_checksum() {
    const ONE_MB: usize = 1024 * 1024;
    let block_size = 4096;

    let mut original: Vec<u8> = vec![0u8; ONE_MB];
    for (i, byte) in original.iter_mut().enumerate() {
        *byte = (i % 256) as u8;
    }

    let mut modified = Vec::with_capacity(ONE_MB + 1);
    modified.push(0xFF);
    modified.extend_from_slice(&original);

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    let data_commands: Vec<_> = delta
        .iter()
        .filter(|cmd| matches!(cmd, DeltaCommand::Data(_)))
        .collect();
    let copy_commands: Vec<_> = delta
        .iter()
        .filter(|cmd| matches!(cmd, DeltaCommand::Copy { .. }))
        .collect();

    assert_eq!(
        data_commands.len(),
        1,
        "Expected exactly 1 Data command for the prepended byte, got {}",
        data_commands.len()
    );

    assert!(
        copy_commands.len() >= 1,
        "Expected at least 1 Copy command, got {}",
        copy_commands.len()
    );

    let total_copy_length: usize = copy_commands
        .iter()
        .map(|cmd| {
            if let DeltaCommand::Copy { length, .. } = cmd {
                *length
            } else {
                0
            }
        })
        .sum();
    assert_eq!(
        total_copy_length, ONE_MB,
        "Total Copy length should equal original data size"
    );

    if let DeltaCommand::Data(data) = &data_commands[0] {
        assert_eq!(data.len(), 1, "Data command should contain only 1 byte");
        assert_eq!(data[0], 0xFF, "Data byte should be 0xFF");
    }

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(&original), &delta, &mut reconstructed).unwrap();

    assert_eq!(
        reconstructed, modified,
        "Reconstructed data should match modified"
    );
}

#[test]
fn test_empty_input() {
    let original = b"some data";
    let modified: &[u8] = b"";

    let signatures = generate_signatures(&original[..]).unwrap();
    let delta = generate_delta(&signatures, &modified[..]).unwrap();

    assert!(delta.is_empty(), "Delta for empty input should be empty");

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_empty_original() {
    let original: &[u8] = b"";
    let modified = b"new data";

    let signatures = generate_signatures(&original[..]).unwrap();
    let delta = generate_delta(&signatures, &modified[..]).unwrap();

    assert_eq!(delta.len(), 1, "Should have exactly 1 Data command");
    assert!(matches!(&delta[0], DeltaCommand::Data(d) if d == modified));

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_data_smaller_than_block_size() {
    let block_size = 1024;

    let original = b"small";
    let modified = b"small";

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_append_data() {
    let block_size = 16;

    let original = b"0123456789ABCDEF";
    let mut modified = original.to_vec();
    modified.extend_from_slice(b"GHIJKLMN");

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    assert_eq!(delta.len(), 2, "Should have Copy + Data commands");
    assert!(matches!(&delta[0], DeltaCommand::Copy { .. }));
    assert!(matches!(&delta[1], DeltaCommand::Data(d) if d == b"GHIJKLMN"));

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_prepend_data() {
    let block_size = 16;

    let original = b"0123456789ABCDEF";
    let mut modified = b"PREFIX__".to_vec();
    modified.extend_from_slice(original);

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    assert_eq!(delta.len(), 2, "Should have Data + Copy commands");
    assert!(matches!(&delta[0], DeltaCommand::Data(d) if d == b"PREFIX__"));
    assert!(matches!(&delta[1], DeltaCommand::Copy { .. }));

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_insert_in_middle() {
    let block_size = 8;

    let original = b"AAAAAAAABBBBBBBB";
    let modified = b"AAAAAAAAXXXXBBBBBBBB";

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_delete_from_middle() {
    let block_size = 8;

    let original = b"AAAAAAAAXXXXXXXXBBBBBBBB";
    let modified = b"AAAAAAAABBBBBBBB";

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_block_reordering() {
    let block_size = 8;

    let original = b"AAAAAAAABBBBBBBBCCCCCCCC";
    let modified = b"CCCCCCCCAAAAAAAABBBBBBBB";

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_duplicate_blocks() {
    let block_size = 8;

    let original = b"AAAAAAAABBBBBBBB";
    let modified = b"AAAAAAAAAAAAAAAABBBBBBBBBBBBBBBB";

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_adjacent_copy_compression() {
    let block_size = 8;

    let original = b"AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDD";
    let modified = original;

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    assert_eq!(
        delta.len(),
        1,
        "Adjacent blocks should be compressed into single Copy command"
    );

    if let DeltaCommand::Copy { offset, length } = &delta[0] {
        assert_eq!(*offset, 0);
        assert_eq!(*length, 32);
    } else {
        panic!("Expected Copy command");
    }

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_non_adjacent_blocks_not_compressed() {
    let block_size = 8;

    let original = b"AAAAAAAABBBBBBBBCCCCCCCC";
    let modified = b"AAAAAAAACCCCCCCC";

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    assert_eq!(
        delta.len(),
        2,
        "Non-adjacent blocks should remain separate Copy commands"
    );

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_large_random_modifications() {
    let block_size = 64;

    let mut original = vec![0u8; 10000];
    let mut seed: u64 = 0x1234_5678;
    for byte in &mut original {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        *byte = (seed >> 56) as u8;
    }

    let mut modified = original.clone();
    modified[500..600].fill(0xFF);
    modified.splice(2000..2000, vec![0xAA; 100]);
    modified.drain(5000..5050);

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(&original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_single_byte_changes() {
    let block_size = 16;

    let original: Vec<u8> = (0..64).collect();
    let mut modified = original.clone();
    modified[0] = 255;
    modified[16] = 255;
    modified[32] = 255;
    modified[48] = 255;

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(&original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_exact_block_boundary() {
    let block_size = 16;

    let original: Vec<u8> = (0..48).collect();
    let modified = original.clone();

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    assert_eq!(delta.len(), 1, "Should be single compressed Copy");

    if let DeltaCommand::Copy { offset, length } = &delta[0] {
        assert_eq!(*offset, 0);
        assert_eq!(*length, 48);
    } else {
        panic!("Expected Copy command");
    }

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(&original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_partial_last_block() {
    let block_size = 16;

    let original: Vec<u8> = (0..50).collect();
    let modified = original.clone();

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(&original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}

#[test]
fn test_entire_block_removed() {
    let block_size = 16;

    let original: Vec<u8> = (0..200).collect();
    let mut modified = original.clone();
    modified.drain(block_size * 4..block_size * 5);

    let signatures = generate_signatures_with_block_size(&original[..], block_size).unwrap();
    let delta = generate_delta_with_block_size(&signatures, &modified[..], block_size).unwrap();

    assert_eq!(delta.len(), 2);
    assert!(
        matches!(&delta[0], DeltaCommand::Copy { offset, length } if *offset == 0 && *length == block_size * 4)
    );
    assert!(
        matches!(&delta[1], DeltaCommand::Copy { offset, length } if *offset == 80 && *length == 120)
    );

    let mut reconstructed = Vec::new();
    apply_delta(Cursor::new(&original), &delta, &mut reconstructed).unwrap();

    assert_eq!(reconstructed, modified);
}
