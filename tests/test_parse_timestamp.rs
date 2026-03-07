#[test]
fn test_parse_fractional_seconds() {
    let base = bvr::analysis::causal::parse_timestamp_ms_pub("2025-01-01T00:00:45Z").unwrap();
    let frac = bvr::analysis::causal::parse_timestamp_ms_pub("2025-01-01T00:00:45.123Z").unwrap();
    assert_eq!(
        base, frac,
        "Expected 45 seconds to be parsed correctly, got {base} != {frac}"
    );
}
