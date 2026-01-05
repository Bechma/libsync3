use libsync3::{DeltaOp, apply_to_vec, delta, signature, signature_with_chunk_size};
use proptest::prelude::*;
use std::io::Cursor;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn roundtrip_identical(data in prop::collection::vec(any::<u8>(), 0..100_000)) {
        let sig = signature(Cursor::new(&data)).unwrap();
        let d = delta(Cursor::new(&data), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(&data), &d).unwrap();
        prop_assert_eq!(&data, &result);
    }

    #[test]
    fn roundtrip_different(
        original in prop::collection::vec(any::<u8>(), 0..50_000),
        modified in prop::collection::vec(any::<u8>(), 0..50_000),
    ) {
        let sig = signature(Cursor::new(&original)).unwrap();
        let d = delta(Cursor::new(&modified), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(&original), &d).unwrap();
        prop_assert_eq!(&modified, &result);
    }

    #[test]
    fn roundtrip_varied_chunk_size(
        original in prop::collection::vec(any::<u8>(), 0..200_000),
        modified in prop::collection::vec(any::<u8>(), 0..200_000),
        chunk_size in (1usize..32).prop_map(|x| x * 256),
    ) {
        let sig = signature_with_chunk_size(Cursor::new(&original), chunk_size).unwrap();
        let d = delta(Cursor::new(&modified), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(&original), &d).unwrap();
        prop_assert_eq!(&modified, &result);
    }

    #[test]
    fn partial_modification(
        base in prop::collection::vec(any::<u8>(), 1000..50_000),
        modify_start in 0usize..1000,
        modify_len in 1usize..500,
        new_bytes in prop::collection::vec(any::<u8>(), 1..500),
    ) {
        let modify_start = modify_start % base.len();
        let modify_end = (modify_start + modify_len).min(base.len());

        let mut modified = base.clone();
        modified.splice(modify_start..modify_end, new_bytes);

        let sig = signature(Cursor::new(&base)).unwrap();
        let d = delta(Cursor::new(&modified), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(&base), &d).unwrap();

        prop_assert_eq!(&modified, &result);
    }

    #[test]
    fn append_data(
        base in prop::collection::vec(any::<u8>(), 100..10_000),
        append in prop::collection::vec(any::<u8>(), 1..5_000),
    ) {
        let mut modified = base.clone();
        modified.extend(&append);

        let sig = signature(Cursor::new(&base)).unwrap();
        let d = delta(Cursor::new(&modified), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(&base), &d).unwrap();

        prop_assert_eq!(&modified, &result);
    }

    #[test]
    fn prepend_data(
        base in prop::collection::vec(any::<u8>(), 100..10_000),
        prepend in prop::collection::vec(any::<u8>(), 1..5_000),
    ) {
        let mut modified = prepend.clone();
        modified.extend(&base);

        let sig = signature(Cursor::new(&base)).unwrap();
        let d = delta(Cursor::new(&modified), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(&base), &d).unwrap();

        prop_assert_eq!(&modified, &result);
    }

    #[test]
    fn truncate_data(
        base in prop::collection::vec(any::<u8>(), 100..50_000),
        keep_ratio in 0.1f64..0.9,
    ) {
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let keep_len = ((base.len() as f64) * keep_ratio) as usize;
        let modified: Vec<u8> = base[..keep_len].to_vec();

        let sig = signature(Cursor::new(&base)).unwrap();
        let d = delta(Cursor::new(&modified), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(&base), &d).unwrap();

        prop_assert_eq!(&modified, &result);
    }
}

// Larger dataset tests (run with --release)
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn large_files(
        original in prop::collection::vec(any::<u8>(), 500_000..1_000_000),
        modified in prop::collection::vec(any::<u8>(), 500_000..1_000_000),
    ) {
        let sig = signature(Cursor::new(&original)).unwrap();
        let d = delta(Cursor::new(&modified), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(&original), &d).unwrap();
        prop_assert_eq!(&modified, &result);
    }

    #[test]
    fn large_similar_files(
        base in prop::collection::vec(any::<u8>(), 500_000..1_000_000),
        modifications in prop::collection::vec((0usize..500_000, any::<u8>()), 10..100),
    ) {
        let mut modified = base.clone();
        for (pos, byte) in modifications {
            let idx = pos % modified.len();
            modified[idx] = byte;
        }

        let sig = signature(Cursor::new(&base)).unwrap();
        let d = delta(Cursor::new(&modified), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(&base), &d).unwrap();

        prop_assert_eq!(&modified, &result);

        // Verify delta is smaller than full modified data for similar files
        let delta_size: usize = d.ops.iter().map(|op| match op {
            DeltaOp::Copy(_) => 8,
            DeltaOp::Insert(data) => data.len() + 8,
        }).sum();
        prop_assert!(delta_size < modified.len(), "Delta size {} should be smaller than original size {}", delta_size, modified.len());

        // Should have some Copy operations for similar data
        let was_copied = d.ops.iter().any(|op| matches!(op, DeltaOp::Copy(_)));
        prop_assert!(was_copied, "Expected some Copy operations for similar files");
    }
}
