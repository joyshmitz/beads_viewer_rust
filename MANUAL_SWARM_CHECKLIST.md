# MANUAL SWARM CHECKLIST

Мінімальний observed-protocol checklist для ручного проходу `bd-bmcf.1.1`
на `acfs` у `beads_viewer_rust`.

## Мета

- Прожити один реальний bead-run руками.
- Зафіксувати тільки ті кроки, які реально виявилися потрібними.
- Не автоматизувати нічого наперед.

## Scope

- Repo: `/data/projects/beads_viewer_rust`
- Branch: `feat/audience-export`
- Bead: `bd-bmcf.1.1`
- Thread convention: `thread_id = bead_id`
- Coordinator єдиний змінює `br`

## 1. Preflight

```bash
cd /data/projects/beads_viewer_rust
git rev-parse --abbrev-ref HEAD
git status --short
br show bd-bmcf.1.1
br ready
export TMPDIR=/data/tmp
rch status
rch workers probe gtt
```

Очікування:
- branch = `feat/audience-export`
- `git status` clean
- `bd-bmcf.1.1` = `OPEN`
- bead є в `br ready`
- `rch` healthy, `gtt` reachable

## 2. Read First

- `AGENTS.md`
- `AUDIENCE_EXPORT_PLAN.md`
- `src/export_pages.rs`
- `tests/export_pages.rs`
- `tests/e2e_export_pages.rs`

## 3. Session Bootstrap

- Підняти `ntm` session з coordinator + worker.
- Обидва агенти перечитують `AGENTS.md`.
- Обидва агенти реєструються в `am`.
- Worker підтверджує своє `agent_name`.

## 4. Pick And Claim

```bash
cd /data/projects/beads_viewer_rust
br show bd-bmcf.1.1
target/debug/bvr --robot-next
br update bd-bmcf.1.1 --status=in_progress
```

Зафіксувати:
- чи збігається `bvr --robot-next` з `bd-bmcf.1.1`
- якщо ні, чому coordinator все одно свідомо вибрав `bd-bmcf.1.1`

## 5. Canonical Packet

Coordinator надсилає packet через Agent Mail.
`ntm send` використовується тільки як poke: перевірити Agent Mail.

Мінімальні поля packet:
- `run_id`
- `bead_id = bd-bmcf.1.1`
- `thread_id = bd-bmcf.1.1`
- `objective`
- `files_to_read_first`
- `files_allowed_to_change`
- `files_out_of_scope`
- `required_verification`
- `stop_conditions`

Для `bd-bmcf.1.1`:
- `objective`: додати regression test, який доводить, що `--export-pages`
  без `--audience` лишається additive-only
- `files_allowed_to_change`:
  - `tests/export_pages.rs`
  - `tests/e2e_export_pages.rs` тільки якщо справді потрібно
- `files_out_of_scope`:
  - `src/export_pages.rs`
  - `src/pages_wizard.rs`
  - `viewer_assets/**`
  - `.beads/**`
- `required_verification`:

```bash
export TMPDIR=/data/tmp
rch exec -- cargo check --all-targets
rch exec -- cargo test --test export_pages
rch exec -- cargo test --test e2e_export_pages
```

- `stop_conditions`:
  - потрібна зміна `src/export_pages.rs`
  - незрозуміло, як відокремити nondeterministic поля
  - поточних export tests недостатньо, щоб виразити additive-only contract

## 6. Worker Execution

- Worker ACK-ає packet у thread `bd-bmcf.1.1`.
- Worker бере reservation тільки на дозволені test files.
- Worker працює тільки в allowed scope.
- Progress update надсилається тільки якщо:
  - треба розширити scope
  - verification падає незрозуміло
  - existing tests недостатньо виражають contract

## 7. Ready For Review

Worker надсилає structured ready-for-review payload:
- `run_id`
- `bead_id`
- `changed_files`
- `commands_run`
- `pass/fail`
- `open_questions`

## 8. Review

Coordinator або reviewer дивиться реальний state:

```bash
cd /data/projects/beads_viewer_rust
git status --short
git diff --stat
git diff -- tests/export_pages.rs tests/e2e_export_pages.rs
```

Перевірити:
- тест справді ловить additive-only regression
- contract покриває:
  - same file tree
  - ordering-invariant JSON
  - byte-identical binary assets
  - intentional nondeterminism excluded
- worker не вийшов за межі дозволеного scope

## 9. Verification Gate

Coordinator rerun-ить authoritative gate:

```bash
cd /data/projects/beads_viewer_rust
export TMPDIR=/data/tmp
rch exec -- cargo check --all-targets
rch exec -- cargo test --test export_pages
rch exec -- cargo test --test e2e_export_pages
```

За потреби:

```bash
rch exec -- cargo fmt --check
rch exec -- cargo clippy --all-targets
```

## 10. Close

Лише після green review + verification:

```bash
cd /data/projects/beads_viewer_rust
git status --short
git add MANUAL_SWARM_CHECKLIST.md tests/export_pages.rs tests/e2e_export_pages.rs
br close bd-bmcf.1.1 --reason "Added additive-only export regression gate"
br sync --flush-only
```

Потім:
- release reservations
- `git add .beads/`
- `git commit`
- `git push`

## 11. Failure / Recovery

Зупинити lane і зафіксувати observation, якщо:
- worker хоче змінювати `src/export_pages.rs`
- `rch` gate нестабільний або flaky
- thread communication ambiguous
- reservation conflict несподіваний
- з diff неможливо зрозуміти, чи additive-only contract реально захищений

## 12. Що записати після проходу

Після manual run зафіксувати тільки observed facts:
- які exact команди були достатні
- який мінімальний packet реально спрацював
- чи вистачило `am` thread + `ntm` poke
- де coordinator застосував judgment
- який був мінімальний working review gate
- який failure або recovery path реально трапився

## 13. Ready Bar Для Першого Проходу

- `bd-bmcf.1.1` пройшов `OPEN -> in_progress -> closed` через `br`
- `thread_id = bd-bmcf.1.1` використовувався послідовно
- packet, ACK, review, verify, close пройдені без grep-евристик
- `rch` був частиною default path
- після проходу стало ясно:
  - що автоматизується
  - що ще лишається ручним judgment
