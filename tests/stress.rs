#[path = "../examples/support/stress.rs"]
mod stress;

#[test]
fn structured_malformed_stream_regression() {
    // Stable seeds cover valid descriptors, damaged real method slices, extreme
    // blob sizes, malformed prefix chains, and private-map/legacy combinations.
    for seed in [1, 0x464e4554, 0xdeadbeef, u64::MAX] {
        assert!(stress::run(1024, seed) > 10_000);
    }
}
