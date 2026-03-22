mod test_utils;

use assert_cmd::Command;
use serde_json::Value;
use std::path::PathBuf;
use test_utils::{JsonType, assert_valid_version_envelope, validate_type_at};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_bvr_json(flags: &[&str], beads_file: &str) -> Value {
    let root = repo_root();
    let beads_path = root.join(beads_file);

    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    let mut command = Command::new(bvr_bin);
    command.args(flags);
    command.arg("--beads-file").arg(&beads_path);

    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("valid JSON output")
}

// ============================================================================
// Schema validation tests for robot output contracts
// ============================================================================

#[test]
fn robot_triage_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-triage"], "tests/testdata/minimal.jsonl");
    // Triage uses older envelope format (no version)
    test_utils::assert_valid_envelope(&output);
    assert!(validate_type_at(&output, "triage", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "triage.quick_ref", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "triage.recommendations", JsonType::Array).is_empty());
}

#[test]
fn robot_plan_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-plan"], "tests/testdata/minimal.jsonl");
    test_utils::assert_valid_envelope(&output);
    assert!(validate_type_at(&output, "plan", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "status", JsonType::Object).is_empty());
}

#[test]
fn robot_insights_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-insights"], "tests/testdata/minimal.jsonl");
    test_utils::assert_valid_envelope(&output);
    assert!(validate_type_at(&output, "status", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "Bottlenecks", JsonType::Array).is_empty());
}

#[test]
fn robot_alerts_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-alerts"], "tests/testdata/minimal.jsonl");
    test_utils::assert_valid_envelope(&output);
    assert!(validate_type_at(&output, "alerts", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "summary", JsonType::Object).is_empty());
}

#[test]
fn robot_suggest_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-suggest"], "tests/testdata/minimal.jsonl");
    test_utils::assert_valid_envelope(&output);
    assert!(validate_type_at(&output, "suggestions", JsonType::Object).is_empty());
}

#[test]
fn robot_capacity_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-capacity"], "tests/testdata/minimal.jsonl");
    assert_valid_version_envelope(&output);
    assert!(validate_type_at(&output, "agents", JsonType::Number).is_empty());
    assert!(validate_type_at(&output, "open_issue_count", JsonType::Number).is_empty());
}

#[test]
fn robot_label_health_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-label-health"], "tests/testdata/minimal.jsonl");
    assert_valid_version_envelope(&output);
    assert!(validate_type_at(&output, "analysis_config", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "results", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "results.total_labels", JsonType::Number).is_empty());
    assert!(validate_type_at(&output, "results.labels", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "results.summaries", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "usage_hints", JsonType::Array).is_empty());
}

#[test]
fn robot_label_flow_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-label-flow"], "tests/testdata/minimal.jsonl");
    assert_valid_version_envelope(&output);
    assert!(validate_type_at(&output, "analysis_config", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "flow", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "flow.labels", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "flow.flow_matrix", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "flow.dependencies", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "usage_hints", JsonType::Array).is_empty());
}

#[test]
fn robot_label_attention_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-label-attention"], "tests/testdata/minimal.jsonl");
    assert_valid_version_envelope(&output);
    assert!(validate_type_at(&output, "labels", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "total_labels", JsonType::Number).is_empty());
    assert!(validate_type_at(&output, "usage_hints", JsonType::Array).is_empty());
}

#[test]
fn robot_label_attention_respects_limit() {
    let output = run_bvr_json(
        &["--robot-label-attention", "--attention-limit", "1"],
        "tests/testdata/synthetic_complex.jsonl",
    );
    assert_valid_version_envelope(&output);
    let labels = output["labels"].as_array().expect("labels array");
    assert!(labels.len() <= 1);
}

#[test]
fn robot_correlation_stats_has_valid_envelope() {
    let output = run_bvr_json(
        &["--robot-correlation-stats"],
        "tests/testdata/minimal.jsonl",
    );
    assert_valid_version_envelope(&output);
    assert!(validate_type_at(&output, "total_feedback", JsonType::Number).is_empty());
    assert!(validate_type_at(&output, "confirmed", JsonType::Number).is_empty());
    assert!(validate_type_at(&output, "rejected", JsonType::Number).is_empty());
    assert!(validate_type_at(&output, "accuracy_rate", JsonType::Number).is_empty());
}

#[test]
fn robot_file_hotspots_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-file-hotspots"], "tests/testdata/minimal.jsonl");
    assert_valid_version_envelope(&output);
    assert!(validate_type_at(&output, "hotspots", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "stats", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "stats.total_files", JsonType::Number).is_empty());
}

#[test]
fn robot_orphans_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-orphans"], "tests/testdata/minimal.jsonl");
    assert_valid_version_envelope(&output);
    assert!(validate_type_at(&output, "stats", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "candidates", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "stats.total_commits", JsonType::Number).is_empty());
}

#[test]
fn robot_search_has_valid_envelope() {
    let output = run_bvr_json(
        &[
            "--robot-search",
            "--search",
            "parity",
            "--search-limit",
            "5",
        ],
        "tests/testdata/synthetic_complex.jsonl",
    );
    assert_valid_version_envelope(&output);
    assert!(validate_type_at(&output, "query", JsonType::String).is_empty());
    assert!(validate_type_at(&output, "limit", JsonType::Number).is_empty());
    assert!(validate_type_at(&output, "mode", JsonType::String).is_empty());
    assert!(validate_type_at(&output, "results", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "usage_hints", JsonType::Array).is_empty());
}

// ============================================================================
// Recently-added field validation (bd-1ru.* parity work)
// ============================================================================

#[test]
fn robot_triage_has_usage_hints() {
    let output = run_bvr_json(&["--robot-triage"], "tests/testdata/minimal.jsonl");
    assert!(validate_type_at(&output, "usage_hints", JsonType::Array).is_empty());
}

#[test]
fn robot_plan_has_analysis_config() {
    let output = run_bvr_json(&["--robot-plan"], "tests/testdata/minimal.jsonl");
    assert!(validate_type_at(&output, "analysis_config", JsonType::Object).is_empty());
}

#[test]
fn robot_plan_has_usage_hints() {
    let output = run_bvr_json(&["--robot-plan"], "tests/testdata/minimal.jsonl");
    assert!(validate_type_at(&output, "usage_hints", JsonType::Array).is_empty());
}

#[test]
fn robot_insights_has_advanced_and_stats_fields() {
    let output = run_bvr_json(
        &["--robot-insights", "--robot-full-stats"],
        "tests/testdata/synthetic_complex.jsonl",
    );
    test_utils::assert_valid_envelope(&output);
    // full_stats is a map of issue-id -> metric node
    assert!(validate_type_at(&output, "full_stats", JsonType::Object).is_empty());
}

#[test]
fn robot_next_has_envelope_and_score_fields() {
    let output = run_bvr_json(&["--robot-next"], "tests/testdata/minimal.jsonl");
    test_utils::assert_valid_envelope(&output);
    // RobotNextOutput always has these fields (nullable)
    assert!(output.get("id").is_some(), "next output must have id field");
    assert!(
        output.get("score").is_some(),
        "next output must have score field"
    );
    assert!(
        output.get("reasons").is_some(),
        "next output must have reasons field"
    );
}

#[test]
fn robot_forecast_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-forecast", "all"], "tests/testdata/minimal.jsonl");
    assert_valid_version_envelope(&output);
    assert!(validate_type_at(&output, "forecasts", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "forecast_count", JsonType::Number).is_empty());
}

#[test]
fn robot_burndown_has_valid_envelope() {
    // --robot-burndown requires a sprint target; use "current" which gracefully
    // returns an error or empty data when no sprints are configured.
    // We test via the binary directly to check the envelope shape.
    let root = repo_root();
    let beads_path = root.join("tests/testdata/minimal.jsonl");
    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    let result = std::process::Command::new(&bvr_bin)
        .args(["--robot-burndown", "current", "--beads-file"])
        .arg(&beads_path)
        .output()
        .expect("failed to run bvr");
    // If it succeeds (sprint found), validate the envelope
    if result.status.success() {
        let output: Value = serde_json::from_slice(&result.stdout).expect("valid JSON output");
        assert_valid_version_envelope(&output);
        assert!(validate_type_at(&output, "daily_points", JsonType::Array).is_empty());
    } else {
        // No sprints configured is expected; just verify it doesn't panic
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            stderr.contains("sprint") || stderr.contains("error"),
            "expected sprint-related error, got: {stderr}"
        );
    }
}

#[test]
fn robot_history_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-history"], "tests/testdata/minimal.jsonl");
    test_utils::assert_valid_envelope(&output);
    // histories is a BTreeMap<String, HistoryBeadCompat> → JSON Object
    assert!(validate_type_at(&output, "histories", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "git_range", JsonType::String).is_empty());
    assert!(validate_type_at(&output, "stats", JsonType::Object).is_empty());
}

#[test]
fn robot_diff_has_valid_envelope() {
    let root = repo_root();
    let beads_path = root.join("tests/testdata/minimal.jsonl");
    // --robot-diff requires --diff-since; use the same file to get an empty diff
    let output = run_bvr_json(
        &["--robot-diff", "--diff-since", beads_path.to_str().unwrap()],
        "tests/testdata/minimal.jsonl",
    );
    test_utils::assert_valid_envelope(&output);
    assert!(output.get("from_data_hash").is_some());
    assert!(output.get("to_data_hash").is_some());
    assert!(output.get("resolved_revision").is_some());
    assert!(validate_type_at(&output, "diff", JsonType::Object).is_empty());
}

#[test]
fn robot_graph_json_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-graph"], "tests/testdata/minimal.jsonl");
    test_utils::assert_valid_envelope(&output);
    // nodes and edges are counts (usize), not arrays
    assert!(validate_type_at(&output, "nodes", JsonType::Number).is_empty());
    assert!(validate_type_at(&output, "edges", JsonType::Number).is_empty());
    assert!(validate_type_at(&output, "format", JsonType::String).is_empty());
    assert!(validate_type_at(&output, "explanation", JsonType::Object).is_empty());
}

#[test]
fn robot_metrics_has_valid_envelope() {
    let output = run_bvr_json(&["--robot-metrics"], "tests/testdata/minimal.jsonl");
    assert_valid_version_envelope(&output);
    // timing, cache are Vec<> → JSON Array
    assert!(validate_type_at(&output, "timing", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "cache", JsonType::Array).is_empty());
    assert!(validate_type_at(&output, "memory", JsonType::Object).is_empty());
}

#[test]
fn robot_search_schema_includes_versioned_envelope_fields() {
    let output = run_bvr_json(
        &["--robot-schema", "--schema-command", "robot-search"],
        "tests/testdata/minimal.jsonl",
    );
    assert!(
        validate_type_at(&output, "schema.properties.output_format", JsonType::Object).is_empty()
    );
    assert!(validate_type_at(&output, "schema.properties.version", JsonType::Object).is_empty());
}

#[test]
fn robot_label_health_schema_includes_versioned_envelope_fields() {
    let output = run_bvr_json(
        &["--robot-schema", "--schema-command", "robot-label-health"],
        "tests/testdata/minimal.jsonl",
    );
    assert!(
        validate_type_at(&output, "schema.properties.output_format", JsonType::Object).is_empty()
    );
    assert!(validate_type_at(&output, "schema.properties.version", JsonType::Object).is_empty());
}

#[test]
fn robot_recipes_schema_includes_versioned_envelope_fields() {
    let output = run_bvr_json(
        &["--robot-schema", "--schema-command", "robot-recipes"],
        "tests/testdata/minimal.jsonl",
    );
    assert!(
        validate_type_at(&output, "schema.properties.output_format", JsonType::Object).is_empty()
    );
    assert!(validate_type_at(&output, "schema.properties.version", JsonType::Object).is_empty());
}

#[test]
fn robot_capacity_schema_includes_versioned_envelope_fields() {
    let output = run_bvr_json(
        &["--robot-schema", "--schema-command", "robot-capacity"],
        "tests/testdata/minimal.jsonl",
    );
    assert!(
        validate_type_at(&output, "schema.properties.output_format", JsonType::Object).is_empty()
    );
    assert!(validate_type_at(&output, "schema.properties.version", JsonType::Object).is_empty());
}

#[test]
fn robot_forecast_schema_includes_versioned_envelope_fields() {
    let output = run_bvr_json(
        &["--robot-schema", "--schema-command", "robot-forecast"],
        "tests/testdata/minimal.jsonl",
    );
    assert!(
        validate_type_at(&output, "schema.properties.output_format", JsonType::Object).is_empty()
    );
    assert!(validate_type_at(&output, "schema.properties.version", JsonType::Object).is_empty());
}

#[test]
fn robot_burndown_schema_includes_versioned_envelope_fields() {
    let output = run_bvr_json(
        &["--robot-schema", "--schema-command", "robot-burndown"],
        "tests/testdata/sprints_synthetic.jsonl",
    );
    assert!(
        validate_type_at(&output, "schema.properties.output_format", JsonType::Object).is_empty()
    );
    assert!(validate_type_at(&output, "schema.properties.version", JsonType::Object).is_empty());
}

#[test]
fn robot_correlation_stats_schema_matches_flattened_output_shape() {
    let output = run_bvr_json(
        &[
            "--robot-schema",
            "--schema-command",
            "robot-correlation-stats",
        ],
        "tests/testdata/minimal.jsonl",
    );
    assert!(
        validate_type_at(
            &output,
            "schema.properties.total_feedback",
            JsonType::Object
        )
        .is_empty()
    );
    assert!(validate_type_at(&output, "schema.properties.confirmed", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "schema.properties.rejected", JsonType::Object).is_empty());
    assert!(
        validate_type_at(&output, "schema.properties.accuracy_rate", JsonType::Object).is_empty()
    );
    assert!(output["schema"]["properties"].get("stats").is_none());
}

#[test]
fn robot_orphans_schema_matches_flattened_output_shape() {
    let output = run_bvr_json(
        &["--robot-schema", "--schema-command", "robot-orphans"],
        "tests/testdata/minimal.jsonl",
    );
    assert!(validate_type_at(&output, "schema.properties.stats", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "schema.properties.candidates", JsonType::Object).is_empty());
    assert!(output["schema"]["properties"].get("report").is_none());
}

#[test]
fn robot_impact_network_schema_matches_flattened_output_shape() {
    let output = run_bvr_json(
        &["--robot-schema", "--schema-command", "robot-impact-network"],
        "tests/testdata/minimal.jsonl",
    );
    assert!(validate_type_at(&output, "schema.properties.bead_id", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "schema.properties.depth", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "schema.properties.network", JsonType::Object).is_empty());
    assert!(
        validate_type_at(&output, "schema.properties.top_connected", JsonType::Object).is_empty()
    );
    assert!(output["schema"]["properties"].get("result").is_none());
}

#[test]
fn robot_drift_schema_matches_flattened_output_shape() {
    let output = run_bvr_json(
        &["--robot-schema", "--schema-command", "robot-drift"],
        "tests/testdata/minimal.jsonl",
    );
    assert!(validate_type_at(&output, "schema.properties.has_drift", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "schema.properties.exit_code", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "schema.properties.summary", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "schema.properties.alerts", JsonType::Object).is_empty());
    assert!(validate_type_at(&output, "schema.properties.baseline", JsonType::Object).is_empty());
    assert!(output["schema"]["properties"].get("result").is_none());
}

// ============================================================================
// Determinism tests for additional robot modes
// ============================================================================

#[test]
fn robot_plan_deterministic() {
    let first = run_bvr_json(&["--robot-plan"], "tests/testdata/minimal.jsonl");
    let second = run_bvr_json(&["--robot-plan"], "tests/testdata/minimal.jsonl");

    let diffs = test_utils::compare_json_ignoring(&first, &second, "", &["generated_at"]);
    assert!(
        diffs.is_empty(),
        "Plan output not deterministic:\n{}",
        test_utils::format_diffs_compact(&diffs)
    );
}

#[test]
fn robot_insights_deterministic() {
    let first = run_bvr_json(&["--robot-insights"], "tests/testdata/minimal.jsonl");
    let second = run_bvr_json(&["--robot-insights"], "tests/testdata/minimal.jsonl");

    let diffs = test_utils::compare_json_ignoring(&first, &second, "", &["generated_at"]);
    assert!(
        diffs.is_empty(),
        "Insights output not deterministic:\n{}",
        test_utils::format_diffs_compact(&diffs)
    );
}

#[test]
fn robot_next_deterministic() {
    let first = run_bvr_json(&["--robot-next"], "tests/testdata/minimal.jsonl");
    let second = run_bvr_json(&["--robot-next"], "tests/testdata/minimal.jsonl");

    let diffs = test_utils::compare_json_ignoring(&first, &second, "", &["generated_at"]);
    assert!(
        diffs.is_empty(),
        "Next output not deterministic:\n{}",
        test_utils::format_diffs_compact(&diffs)
    );
}

#[test]
fn robot_alerts_deterministic() {
    let first = run_bvr_json(&["--robot-alerts"], "tests/testdata/minimal.jsonl");
    let second = run_bvr_json(&["--robot-alerts"], "tests/testdata/minimal.jsonl");

    let diffs = test_utils::compare_json_ignoring(&first, &second, "", &["generated_at"]);
    assert!(
        diffs.is_empty(),
        "Alerts output not deterministic:\n{}",
        test_utils::format_diffs_compact(&diffs)
    );
}

#[test]
fn robot_capacity_deterministic() {
    let first = run_bvr_json(&["--robot-capacity"], "tests/testdata/minimal.jsonl");
    let second = run_bvr_json(&["--robot-capacity"], "tests/testdata/minimal.jsonl");

    let diffs = test_utils::compare_json_ignoring(&first, &second, "", &["generated_at"]);
    assert!(
        diffs.is_empty(),
        "Capacity output not deterministic:\n{}",
        test_utils::format_diffs_compact(&diffs)
    );
}

// ============================================================================
// Complex fixture validates richer output shapes
// ============================================================================

#[test]
fn robot_triage_complex_has_recommendations() {
    let output = run_bvr_json(
        &["--robot-triage"],
        "tests/testdata/synthetic_complex.jsonl",
    );
    test_utils::assert_valid_envelope(&output);
    let recs = output["triage"]["recommendations"]
        .as_array()
        .expect("recommendations array");
    assert!(
        !recs.is_empty(),
        "complex fixture must produce triage recommendations"
    );
}

#[test]
fn robot_plan_complex_has_tracks() {
    let output = run_bvr_json(&["--robot-plan"], "tests/testdata/synthetic_complex.jsonl");
    test_utils::assert_valid_envelope(&output);
    let tracks = output["plan"]["tracks"].as_array().expect("tracks array");
    assert!(
        !tracks.is_empty(),
        "complex fixture must produce execution tracks"
    );
}

#[test]
fn robot_insights_complex_has_bottlenecks() {
    let output = run_bvr_json(
        &["--robot-insights"],
        "tests/testdata/synthetic_complex.jsonl",
    );
    test_utils::assert_valid_envelope(&output);
    let bottlenecks = output["Bottlenecks"]
        .as_array()
        .expect("bottlenecks array");
    assert!(
        !bottlenecks.is_empty(),
        "complex fixture must produce bottleneck insights"
    );
}

// ============================================================================
// Comparator self-tests (verifying the test_utils module itself)
// ============================================================================

#[test]
fn comparator_detects_field_drift() {
    let expected = serde_json::json!({
        "generated_at": "2026-03-04T07:00:00Z",
        "data_hash": "abc123",
        "output_format": "json",
        "version": "v0.1.0",
        "total": 5,
        "items": [{"id": "A"}, {"id": "B"}]
    });
    let actual = serde_json::json!({
        "generated_at": "2026-03-04T08:00:00Z",
        "data_hash": "abc123",
        "output_format": "json",
        "version": "v0.1.0",
        "total": 5,
        "items": [{"id": "A"}, {"id": "B"}]
    });

    // Without ignoring: should find 1 diff (generated_at)
    let diffs = test_utils::compare_json(&expected, &actual, "", None);
    assert_eq!(diffs.len(), 1);

    // Ignoring generated_at: should be clean
    let diffs = test_utils::compare_json_ignoring(&expected, &actual, "", &["generated_at"]);
    assert!(diffs.is_empty());
}

#[test]
fn comparator_order_invariant_arrays() {
    let expected = serde_json::json!([
        {"id": "C", "score": 3},
        {"id": "A", "score": 1},
        {"id": "B", "score": 2}
    ]);
    let actual = serde_json::json!([
        {"id": "A", "score": 1},
        {"id": "B", "score": 2},
        {"id": "C", "score": 3}
    ]);

    // Strict: should differ (order matters)
    let strict = test_utils::compare_json(&expected, &actual, "", None);
    assert!(!strict.is_empty());

    // Sorted by id: should match
    let sorted = test_utils::compare_json(&expected, &actual, "", Some("id"));
    assert!(sorted.is_empty());
}

#[test]
fn robot_triage_deterministic() {
    let first = run_bvr_json(&["--robot-triage"], "tests/testdata/minimal.jsonl");
    let second = run_bvr_json(&["--robot-triage"], "tests/testdata/minimal.jsonl");

    let diffs = test_utils::compare_json_ignoring(&first, &second, "", &["generated_at"]);
    assert!(
        diffs.is_empty(),
        "Triage output not deterministic:\n{}",
        test_utils::format_diffs_compact(&diffs)
    );
}

#[test]
fn robot_label_health_deterministic() {
    let first = run_bvr_json(&["--robot-label-health"], "tests/testdata/minimal.jsonl");
    let second = run_bvr_json(&["--robot-label-health"], "tests/testdata/minimal.jsonl");

    let diffs = test_utils::compare_json_ignoring(
        &first,
        &second,
        "",
        &["generated_at", "most_recent_update", "oldest_open_issue"],
    );
    assert!(
        diffs.is_empty(),
        "Label health output not deterministic:\n{}",
        test_utils::format_diffs_compact(&diffs)
    );
}
