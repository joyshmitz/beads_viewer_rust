# Приклади споживачів робот-контрактів `bvr`

Ця директорія демонструє архітектурну тезу, до якої ми прийшли в issue #12: `bvr --robot-*` примітиви є **інтеграційними контрактами**, а людські лінзи й зовнішні інтеграції живуть нижче за течією. Тут нема жодного HTML-рендерера всередині самого `bvr`; усі приклади читають стабільні JSON-envelope-и й самі вирішують, як рендерити для свого читача.

## Структура

```
examples/contract-consumers/
├── triad.sh                    # оркестратор: запускає три примітиви, кешує JSON
├── economics.sample.json       # приклад overlay для --robot-economics (скопіюй у .bv/economics.json)
├── lenses/
│   ├── engineer/brief.sh       # термінал-брифінг для розробника
│   ├── owner/brief.sh          # delivery-posture для керівника
│   ├── investor/brief.sh       # фінансовий вигляд з provenance
│   └── erp/adapter.jq          # нормалізатор у finance-schema
├── portfolio/
│   └── rollup.sh               # агрегатор по N проєктах
└── README.md                   # цей файл

.bv/                            # local-only (gitignored)
├── economics.json              # твоя копія overlay (потрібна для --robot-economics)
└── runs/                       # кеш --robot-* виходів (регенерується triad.sh)
    ├── overview.json
    ├── delivery.json
    └── economics.json
```

## Швидкий запуск

З кореня репозиторію:

```bash
# 1. Зібрати bvr
rch exec -- cargo build --release --bin bvr

# 2. Підготувати локальний overlay (одноразово)
mkdir -p .bv && cp examples/contract-consumers/economics.sample.json .bv/economics.json
# (відредагуй .bv/economics.json зі своїми ставками)

# 3. Згенерувати тріаду (записує у .bv/runs/)
examples/contract-consumers/triad.sh

# 4. Запустити лінзи над кешованими JSON
examples/contract-consumers/lenses/engineer/brief.sh    # для себе
examples/contract-consumers/lenses/owner/brief.sh       # для delivery-lead
examples/contract-consumers/lenses/investor/brief.sh    # для фінансів
jq -f examples/contract-consumers/lenses/erp/adapter.jq --arg project bvr .bv/runs/economics.json
```

Для портфоліо (коли будуть кілька репо):

```bash
examples/contract-consumers/portfolio/rollup.sh /path/to/proj-a /path/to/proj-b /path/to/proj-c
```

## Що кожна лінза робить із тими самими фактами

Усі чотири споживачі читають один той самий `data_hash`. Різниця у тому, **як** вони говорять про нього.

- **Engineer** — орієнтований на дію: наступний крок, топ-блокери за `dependents_count`, які guards спрацювали. Компактно, з `claim_command` готовим до копіпасту.
- **Owner** — орієнтований на агрегати: розподіл flow, когорти urgency, milestone pressure. Жодних per-issue ID; керівник не клеймить задачі.
- **Investor** — орієнтований на числа і provenance: input assumptions, проєкції, guards як data-quality флаги (`TRIPPED` / `ok`), явний блок provenance із `data_hash` і `overlay_hash`. Навмисно без ярликів «on track» / «at risk» — downstream-consumer обирає власні пороги.
- **ERP** — орієнтований на transport-shape: нормалізована JSON-структура зі snake_case полями, currency на верхньому рівні, `guards_tripped` як плоский список, provenance як окремий блок. Finance-система приймає цей JSON без знання про beads чи bvr.

## Чому це доводить правильність архітектурного ходу issue #12

Якщо economics чи delivery жили б у вигляді HTML-примітиву всередині `bvr` (як пропонував оригінальний AUDIENCE_EXPORT_PLAN), кожен із цих чотирьох споживачів мусив би парсити HTML або запускати свій власний дублюючий obчислювач. Натомість усі вони жевуть з одного payload-у, і наступний читач (Slack-дайджест, compliance-архів, CI-гейт) додається без зміни `bvr`.

Портфоліо-rollup робить це явним: N проєктів × `--robot-economics` стає одним агрегатом простою арифметикою поверх payload-ів. HTML-лінзи не складаються так.

## Що ці приклади **не** є

- Не production-ready. Shell + jq — це демонстрація патерну; реальна finance-інтеграція зазвичай пише адаптер на Go/Python/TypeScript із валідацією схеми й retry-логікою.
- Не єдино-правильні лінзи. «Investor» тут умовний; справжній investor-view може вимагати додаткових полів (runway, burn multiple, ratio), яких у поточному контракті немає. Це нормально: новий лінзо-споживач додає потрібні обчислення на своєму боці, не форкаючи `bvr`.
- Не готові інтеграції з ERP. `erp/adapter.jq` повертає зразок схеми; реальна інтеграція мапить на конкретні поля SAP / NetSuite / QuickBooks / будь-чого, що приймає integrator.

## Розширення

Мапа напрямків, куди ці приклади можуть розвинутись (тема паралельного дослідження оператора):

- **Програміст** (engineer): можна додати git-blame-adjusted prompt, який автоматично теги issues з обмеженою людською увагою.
- **Керівник** (owner / delivery lead): PMO-дашборд на CSS-only HTML, що рендериться з `delivery.json` + `economics.json` нічним cron-ом.
- **Інвестор** (finance / board): PDF-звіт із тих самих двох файлів + `overlay_hash` як аудит-анкером.
- **ERP**: реальний адаптер у формат, який приймає SAP S/4HANA / NetSuite / внутрішня OLAP-система.
- **Compliance / audit**: git-checkout за ref + повторний `triad.sh` = реконструкція стану на будь-яку дату.
- **Multi-agent swarm coordinator**: читає `cost_of_delay[]` як circuit-breaker і перерозподіляє NTM-agents за пріоритетом розблокування.
