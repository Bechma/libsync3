#![cfg(feature = "serde")]

use libsync3::{
    Delta, DeltaOp, LightweightHash, LightweightSignature, Signature, lightweight_signature,
    signature,
};
use std::io::Cursor;

#[test]
fn test_signature_serde() {
    let data = b"Hello, world! This is a test for serde serialization.";
    let sig = signature(Cursor::new(data)).unwrap();

    let json = serde_json::to_string(&sig).unwrap();
    let deserialized: Signature = serde_json::from_str(&json).unwrap();

    assert_eq!(sig.chunk_size, deserialized.chunk_size);
    assert_eq!(sig.chunks.len(), deserialized.chunks.len());

    for (original, deserialized) in sig.chunks.iter().zip(deserialized.chunks.iter()) {
        assert_eq!(original.index, deserialized.index);
        assert_eq!(original.hash, deserialized.hash);
    }
}

#[test]
fn test_lightweight_signature_serde() {
    let data = b"Hello, world! This is a test for lightweight serde serialization.";
    let sig = lightweight_signature(Cursor::new(data)).unwrap();

    let json = serde_json::to_string(&sig).unwrap();
    let deserialized: LightweightSignature = serde_json::from_str(&json).unwrap();

    assert_eq!(sig.chunk_size, deserialized.chunk_size);
    assert_eq!(sig.chunks.len(), deserialized.chunks.len());

    for (original, deserialized) in sig.chunks.iter().zip(deserialized.chunks.iter()) {
        assert_eq!(original.index, deserialized.index);
        assert_eq!(original.hash, deserialized.hash);
    }
}

#[test]
fn test_delta_serde() {
    let old_data = b"Hello, world!";
    let new_data = b"Hello, Rust world!";

    let sig = signature(Cursor::new(old_data)).unwrap();
    let delta = libsync3::delta(Cursor::new(new_data), &sig).unwrap();

    let json = serde_json::to_string(&delta).unwrap();
    let deserialized: Delta = serde_json::from_str(&json).unwrap();

    assert_eq!(delta.chunk_size, deserialized.chunk_size);
    assert_eq!(delta.final_size, deserialized.final_size);
    assert_eq!(delta.ops.len(), deserialized.ops.len());
}

#[test]
fn test_lightweight_hash_serde() {
    let data = b"test data";
    let hash = LightweightHash::new(data);

    let json = serde_json::to_string(&hash).unwrap();
    let deserialized: LightweightHash = serde_json::from_str(&json).unwrap();

    assert_eq!(hash, deserialized);
    assert_eq!(hash.as_u64(), deserialized.as_u64());
}

#[test]
fn test_delta_ops_serde() {
    let copy_op = DeltaOp::Copy(42);
    let insert_op = DeltaOp::Insert(vec![1, 2, 3, 4, 5]);

    let copy_json = serde_json::to_string(&copy_op).unwrap();
    let insert_json = serde_json::to_string(&insert_op).unwrap();

    let copy_deserialized: DeltaOp = serde_json::from_str(&copy_json).unwrap();
    let insert_deserialized: DeltaOp = serde_json::from_str(&insert_json).unwrap();

    match (copy_op, copy_deserialized) {
        (DeltaOp::Copy(a), DeltaOp::Copy(b)) => assert_eq!(a, b),
        _ => panic!("Copy operation not deserialized correctly"),
    }

    match (insert_op, insert_deserialized) {
        (DeltaOp::Insert(a), DeltaOp::Insert(b)) => assert_eq!(a, b),
        _ => panic!("Insert operation not deserialized correctly"),
    }
}

#[test]
fn test_roundtrip_with_serde() {
    let old_data = b"The quick brown fox jumps over the lazy dog.";
    let new_data = b"The quick brown fox leaps over the lazy cat.";

    let sig = signature(Cursor::new(old_data)).unwrap();
    let delta = libsync3::delta(Cursor::new(new_data), &sig).unwrap();

    let sig_json = serde_json::to_string(&sig).unwrap();
    let delta_json = serde_json::to_string(&delta).unwrap();

    let _sig_restored: Signature = serde_json::from_str(&sig_json).unwrap();
    let delta_restored: Delta = serde_json::from_str(&delta_json).unwrap();

    let result = libsync3::apply_to_vec(Cursor::new(old_data), &delta_restored).unwrap();
    assert_eq!(result, new_data);
}

#[test]
fn test_lightweight_roundtrip_with_serde() {
    let old_data = b"The quick brown fox jumps over the lazy dog.";
    let new_data = b"The quick brown fox leaps over the lazy cat.";

    let sig = lightweight_signature(Cursor::new(old_data)).unwrap();
    let delta = libsync3::lightweight_delta(Cursor::new(new_data), &sig).unwrap();

    let sig_json = serde_json::to_string(&sig).unwrap();
    let delta_json = serde_json::to_string(&delta).unwrap();

    let _sig_restored: LightweightSignature = serde_json::from_str(&sig_json).unwrap();
    let delta_restored: Delta = serde_json::from_str(&delta_json).unwrap();

    let result = libsync3::apply_to_vec(Cursor::new(old_data), &delta_restored).unwrap();
    assert_eq!(result, new_data);
}
