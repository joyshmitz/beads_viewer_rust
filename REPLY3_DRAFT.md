All three contract-critical pieces landed as framed: `schema_version` as a payload field, cross-surface coherence pinned via `cost_of_delay_ids_match_top_bottlenecks_for_cross_surface_coherence`, structural determinism as a named regression. The strawman's point was to make those decisions explicit before code; honoring them unchanged is the validation.

Two things the implementation improved on the strawman:

- **P0-with-due_date resolution.** Priority wins so Expedite isn't silently depopulated by committed work — good catch on an edge case we didn't discuss.
- **Two-method `cost_to_complete`.** Estimate-based when coverage ≥ 50%, throughput fallback otherwise, `null` only when both guards trip. Good resolution of an ambiguity the strawman left unspecified.

Overlay shape shipped as `--economics-overlay <path.json>` + `BVR_ECONOMICS_OVERLAY` env var rather than `.bvr/economics.yaml` directory discovery. Explicit-path + env var composes better for CI gates, container deployments, and agent-swarm contexts than directory-lookup would have — reasonable divergence, noted for the record.

Closing on our side.
