Thanks for the thorough triage note — ready for all three direction calls. The `Issue`-field table resolves the class of "needs a loader/schema migration" objection I had implicitly worried about; both primitives are pure consumers of `Issue` fields already handled by the existing JSONL deserialization path.

## On (1) — overlay location and format

**Agree with (a): `.bvr/economics.yaml`**, with two refinements:

- **CLI flags as optional overrides, not replacements.** Keep `.bvr/economics.yaml` as the canonical source; allow `--hourly-rate` / `--hours-per-day` / `--budget-envelope` as ad-hoc overrides for one-off runs (experiments, what-if sweeps). Documents the intent ("workspace config") without losing the scratch-pad case.
- **`overlay_hash` must hash the effective merged config used for the projection**, not just the file on disk. Otherwise `.bvr/economics.yaml` unchanged + different CLI overrides = different projection numbers with identical `overlay_hash` — silent provenance corruption. Hash of the final value set is the honest contract.

Rationale for (a) over (b) / (c):

- Economics inputs are workspace-scoped (rate per team, budget per project) — directory-level config matches natural granularity.
- `.bvr/` as sibling to `.beads/` follows the per-project directory-as-config pattern already in use for issue data.
- Git-versioned overlay → git-based historical replay works for economics, which was a non-goal's implicit assumption (`git checkout <ref> && bvr --robot-economics`).
- (c) (CLI-only) fails the replay test: CLI values aren't recoverable from `git log`.

## On (2) — classification rules for `--robot-delivery`

Your tables are the right v0.1.0 starting point — predictable, user-controlled, no dependency on analyzer-pass ordering. Counter-proposal is narrow: **extend the default match rules to also pick up analyzer signals where the signal already exists**. Two tiers gain from it; two don't. This keeps the rules non-overlayable in v0.1.0 (per your "agree on defaults first" principle) — just makes the defaults richer.

**`flow_distribution`** — layered OR:

| Tier | Match rule (counter-proposal) |
|---|---|
| Risk | `issue_type == "risk"` OR labels contain `risk`/`safety`/`security` OR **has a `critical`-level `Alert` from `--robot-alerts`** |
| Debt | labels contain `tech-debt`/`refactor`/`cleanup` OR `issue_type == "chore"` OR **has a `CycleWarning` or `StaleCleanup` `Suggestion` from `--robot-suggest`, or an entry in `--robot-orphans`** |
| Defects | `issue_type == "bug"` |
| Features | default fall-through |

Gains: a user who hasn't labeled a bad cycle still sees it count as Debt; an analyzer-detected critical alert on an unlabeled issue still lands in Risk. No new analysis pipeline — both outputs are produced on demand by existing `analyzer.alerts()` / `analyzer.suggest()` methods (`src/analysis/mod.rs:638, 643`); `--robot-delivery` would call those same methods, not introduce new graph analysis. The primitive stays coherent with the rest of the `--robot-*` surface rather than being a label-only silo.

**`urgency_profile`** — accept your table, with one addition on Intangible:

| Tier | Match rule |
|---|---|
| Expedite | `priority == 0` OR labels contain `expedite`/`incident` (accept) |
| Fixed-Date | `due_date.is_some() && due_date <= now + 7d` (accept) |
| Intangible | labels contain `research`/`spike`/`exploratory` OR **has a `StaleCleanup` `Suggestion` from `--robot-suggest`** |
| Standard | default |

Two notes:

- **P0 as highest is bvr's actual convention** (`priority_normalized_maps_p0_to_highest_and_p4_to_lowest` in `src/model.rs:451`). Accepting your `priority == 0` as the correct anchor for Expedite.
- **Planning horizon = 7d** is a fine default. Worth considering overlayable in a future version (sprint-based vs milestone-based teams will want different), but for v0.1.0 lock it at 7d — avoids a second overlay decision.

**Scope clarification worth pinning on the contract**: both `flow_distribution` and `urgency_profile` operate over **open-like work** per bvr's existing `is_open_like()` convention (`src/model.rs:168`) — i.e., everything not closed / tombstone, with `deferred` counted as open-like as the current model has it. `percentages sum to 100%` across that pool only. Prevents a class of "why does %debt go down when we close a feature" confusion downstream. If deferred-exclusion ever becomes a wanted variant, that's a later overlay decision, not a v0.1.0 default.

## On (3) — schema-version migration

Accept your lean unchanged: **never break, always additive, bump on additive**. Same policy as the envelope. Justification is the one you named — downstream pins `schema_version: 1` and uses field-presence checks for new features. Bumping on additive too means the consumer's version check is load-bearing rather than a false negative on silent schema growth.

One implementation detail: expose this as a `schema_policy: "additive-only"` field in the `--robot-schema` output for these primitives, so agents consuming the schema don't have to read a CONTRIBUTING.md to learn the rule. One line of docs in the contract itself.

## Phasing

Accept: `--robot-delivery` first, `--robot-economics` after. No objection.

One addition: `--robot-delivery`'s default classification tables are the piece of observable contract that ships in v0.1.0 without ability to be overlay-shadowed later. Lock them as canonical output, and position the potential `.bvr/delivery_rules.yaml` escape hatch (from your earlier note) as strictly optional in a later version — consumers pin against defaults, not against user-overridable rules. Same principle you raised: agree on defaults first, add configurability only if real usage shows it's needed.

## Output format

Minor but worth confirming: both primitives emit under JSON and TOON via the existing `--format` flag, with the same envelope-flattening convention. No new serializer path.

## Ready to move

With (1) / (2) / (3) pinned — `.bvr/economics.yaml` with CLI overrides and hash-of-effective-config, layered OR classification defaults, additive-only schema versioning — there are no remaining open blockers from the original issue. Happy to keep this thread open for reference until both primitives register under `--robot-docs`.
