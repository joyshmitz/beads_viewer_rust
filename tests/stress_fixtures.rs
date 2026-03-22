//! Stress tests exercising large-dataset and adversarial fixtures.
//!
//! Validates that bvr handles:
//! - 500-issue graphs with diverse topologies (chains, hubs, diamonds, cycles)
//! - Pathological dependency patterns (deep chains, convergence, divergence, overlapping cycles, self-deps)
//! - Malformed/edge-case metadata (empty strings, negative priority, unicode, huge estimates)
//!
//! Each fixture must load, parse, and produce valid JSON output without panics.

mod test_utils;

use assert_cmd::Command;
use serde_json::Value;

fn bvr() -> Command {
    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    Command::new(bvr_bin)
}

const LARGE_500: &str = "tests/testdata/stress_large_500.jsonl";
const PATHOLOGICAL: &str = "tests/testdata/pathological_deps.jsonl";
const MALFORMED: &str = "tests/testdata/malformed_metadata.jsonl";

/// Run a robot command against a fixture and return parsed JSON.
fn run_robot(args: &[&str], fixture: &str) -> Value {
    let output = bvr()
        .args(args)
        .arg("--beads-file")
        .arg(fixture)
        .output()
        .expect("failed to execute bvr");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Command {:?} against {fixture} failed (exit={}).\nstdout: {stdout}\nstderr: {stderr}",
        args,
        output.status.code().unwrap_or(-1)
    );

    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("Command {args:?} against {fixture} produced invalid JSON: {e}\nOutput: {stdout}")
    })
}

// =========================================================================
// stress_large_500.jsonl — 500-issue diverse topology graph
// =========================================================================

#[test]
fn stress_large_500_triage() {
    let json = run_robot(&["--robot-triage"], LARGE_500);
    let triage = &json["triage"];
    assert!(!triage["recommendations"].as_array().unwrap().is_empty());
    assert!(triage["quick_ref"]["total_open"].as_u64().unwrap() > 100);
}

#[test]
fn stress_large_500_insights() {
    let json = run_robot(&["--robot-insights"], LARGE_500);
    let insights = &json;
    // Should detect the 5 cycle groups
    let cycles = insights["Cycles"].as_array().unwrap();
    assert!(
        cycles.len() >= 5,
        "Expected at least 5 cycles, got {}",
        cycles.len()
    );
}

#[test]
fn stress_large_500_next() {
    let json = run_robot(&["--robot-next"], LARGE_500);
    assert!(json["id"].is_string());
    assert!(json["score"].is_f64() || json["score"].is_u64());
}

#[test]
fn stress_large_500_graph() {
    let json = run_robot(&["--robot-graph"], LARGE_500);
    assert_eq!(
        json["nodes"].as_u64().unwrap(),
        500,
        "Should have 500 graph nodes"
    );
    assert!(
        json["edges"].as_u64().unwrap() > 100,
        "Should have many edges"
    );
}

#[test]
fn stress_large_500_plan() {
    let json = run_robot(&["--robot-plan"], LARGE_500);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_large_500_suggest() {
    let json = run_robot(&["--robot-suggest"], LARGE_500);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_large_500_alerts() {
    let json = run_robot(&["--robot-alerts"], LARGE_500);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_large_500_history() {
    let json = run_robot(&["--robot-history"], LARGE_500);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_large_500_metrics() {
    let json = run_robot(&["--robot-metrics"], LARGE_500);
    assert!(json["data_hash"].is_string());
}

#[test]
fn stress_large_500_label_health() {
    let json = run_robot(&["--robot-label-health"], LARGE_500);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_large_500_label_flow() {
    let json = run_robot(&["--robot-label-flow"], LARGE_500);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_large_500_label_attention() {
    let json = run_robot(&["--robot-label-attention"], LARGE_500);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_large_500_search() {
    let json = run_robot(&["--robot-search", "--search", "chain"], LARGE_500);
    let results = json["results"].as_array().unwrap();
    assert!(!results.is_empty(), "Should find chain issues");
}

#[test]
fn stress_large_500_diff() {
    let json = run_robot(
        &[
            "--robot-diff",
            "--diff-since",
            "tests/testdata/minimal.jsonl",
        ],
        LARGE_500,
    );
    assert!(json["diff"].is_object());
}

#[test]
fn stress_large_500_forecast() {
    let json = run_robot(&["--robot-forecast", "7"], LARGE_500);
    assert!(json["forecasts"].is_array());
}

#[test]
fn stress_large_500_capacity() {
    let json = run_robot(&["--robot-capacity", "--agents", "3"], LARGE_500);
    assert!(json["total_minutes"].is_number());
}

#[test]
fn stress_large_500_profile() {
    let json = run_robot(&["--profile-startup", "--profile-json"], LARGE_500);
    let profile = &json["profile"];
    assert_eq!(profile["node_count"].as_u64().unwrap(), 500);
}

// =========================================================================
// pathological_deps.jsonl — Extreme dependency patterns
// =========================================================================

#[test]
fn stress_pathological_triage() {
    let json = run_robot(&["--robot-triage"], PATHOLOGICAL);
    assert!(json["triage"]["recommendations"].is_array());
}

#[test]
fn stress_pathological_insights_detects_cycles() {
    let json = run_robot(&["--robot-insights"], PATHOLOGICAL);
    let insights = &json;
    let cycles = insights["Cycles"].as_array().unwrap();
    // Overlapping cycles + bidirectional + long cycle + self-dep = many cycles expected
    assert!(
        cycles.len() >= 3,
        "Expected at least 3 cycles in pathological fixture, got {}",
        cycles.len()
    );
}

#[test]
fn stress_pathological_graph_deep_chain() {
    let json = run_robot(&["--robot-graph"], PATHOLOGICAL);
    let node_count = json["nodes"].as_u64().unwrap();
    assert!(node_count >= 200, "Expected 200+ nodes, got {node_count}");
}

#[test]
fn stress_pathological_bottleneck_convergence() {
    // The convergence sink (PD-150) depends on 50 issues — should be flagged
    let json = run_robot(&["--robot-insights"], PATHOLOGICAL);
    let insights = &json;
    let bottlenecks = insights["Bottlenecks"].as_array().unwrap();
    assert!(
        !bottlenecks.is_empty(),
        "Convergence bottleneck should be detected"
    );
}

#[test]
fn stress_pathological_suggest() {
    let json = run_robot(&["--robot-suggest"], PATHOLOGICAL);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_pathological_alerts() {
    let json = run_robot(&["--robot-alerts"], PATHOLOGICAL);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_pathological_search_cycle() {
    let json = run_robot(&["--robot-search", "--search", "cycle"], PATHOLOGICAL);
    let results = json["results"].as_array().unwrap();
    assert!(!results.is_empty(), "Should find cycle-related issues");
}

#[test]
fn stress_pathological_dangling_dep() {
    // PD-250 depends on non-existent GHOST-999 — should not crash
    let json = run_robot(&["--robot-triage"], PATHOLOGICAL);
    assert!(json["data_hash"].is_string());
}

#[test]
fn stress_pathological_self_dep() {
    // PD-210 depends on itself — should not crash
    let json = run_robot(&["--robot-graph"], PATHOLOGICAL);
    assert!(json["nodes"].is_u64());
}

// =========================================================================
// malformed_metadata.jsonl — Edge-case metadata values
// =========================================================================

#[test]
fn stress_malformed_triage() {
    let json = run_robot(&["--robot-triage"], MALFORMED);
    assert!(json["data_hash"].is_string());
}

#[test]
fn stress_malformed_insights() {
    let json = run_robot(&["--robot-insights"], MALFORMED);
    assert!(json["data_hash"].is_string());
}

#[test]
fn stress_malformed_graph() {
    let json = run_robot(&["--robot-graph"], MALFORMED);
    let node_count = json["nodes"].as_u64().unwrap();
    // Some malformed issues may be filtered; accept any count > 0
    assert!(node_count > 0, "Should have some graph nodes");
}

#[test]
fn stress_malformed_search_unicode() {
    let json = run_robot(&["--robot-search", "--search", "Ünïcödé"], MALFORMED);
    let results = json["results"].as_array().unwrap();
    assert!(!results.is_empty(), "Should find unicode-titled issues");
}

#[test]
fn stress_malformed_suggest() {
    let json = run_robot(&["--robot-suggest"], MALFORMED);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_malformed_alerts() {
    let json = run_robot(&["--robot-alerts"], MALFORMED);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_malformed_history() {
    let json = run_robot(&["--robot-history"], MALFORMED);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_malformed_label_health() {
    let json = run_robot(&["--robot-label-health"], MALFORMED);
    assert!(json["generated_at"].is_string());
}

#[test]
fn stress_malformed_profile() {
    let json = run_robot(&["--profile-startup", "--profile-json"], MALFORMED);
    let profile = &json["profile"];
    // Some malformed issues may not produce graph nodes
    assert!(profile["node_count"].as_u64().unwrap() > 0);
}
