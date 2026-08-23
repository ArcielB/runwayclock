PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS accounts (
    id                  INTEGER PRIMARY KEY,
    external_key        TEXT NOT NULL UNIQUE,
    display_name        TEXT NOT NULL,
    currency            TEXT NOT NULL CHECK(length(currency) = 3),
    created_at           TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS import_profiles (
    id                  INTEGER PRIMARY KEY,
    name                TEXT NOT NULL UNIQUE,
    config_toml         TEXT NOT NULL,
    updated_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS import_batches (
    id                  INTEGER PRIMARY KEY,
    account_id          INTEGER NOT NULL REFERENCES accounts(id),
    profile_id          INTEGER NOT NULL REFERENCES import_profiles(id),
    source_name         TEXT NOT NULL,
    content_sha256      TEXT NOT NULL,
    profile_sha256      TEXT NOT NULL,
    profile_config_toml TEXT NOT NULL,
    imported_at         TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    row_count           INTEGER NOT NULL DEFAULT 0,
    inserted_count      INTEGER NOT NULL DEFAULT 0,
    duplicate_count     INTEGER NOT NULL DEFAULT 0,
    error_count         INTEGER NOT NULL DEFAULT 0,
    UNIQUE(account_id, content_sha256, profile_sha256)
);

CREATE TABLE IF NOT EXISTS actual_transactions (
    id                      INTEGER PRIMARY KEY,
    account_id              INTEGER NOT NULL REFERENCES accounts(id),
    booked_on               TEXT NOT NULL,
    description_raw         TEXT NOT NULL,
    description_normalized  TEXT NOT NULL,
    amount_minor            INTEGER NOT NULL,
    currency                TEXT NOT NULL CHECK(length(currency) = 3),
    balance_after_minor     INTEGER,
    external_id             TEXT,
    dedup_key               TEXT NOT NULL,
    source_batch_id         INTEGER NOT NULL REFERENCES import_batches(id),
    created_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(account_id, dedup_key)
);

CREATE INDEX IF NOT EXISTS actual_transactions_by_date
    ON actual_transactions(account_id, booked_on, id);

CREATE TABLE IF NOT EXISTS raw_import_rows (
    id                  INTEGER PRIMARY KEY,
    batch_id            INTEGER NOT NULL REFERENCES import_batches(id),
    row_number          INTEGER NOT NULL,
    raw_json            TEXT NOT NULL,
    parse_error         TEXT,
    transaction_id      INTEGER REFERENCES actual_transactions(id),
    UNIQUE(batch_id, row_number)
);

CREATE TABLE IF NOT EXISTS transaction_interpretations (
    transaction_id      INTEGER PRIMARY KEY REFERENCES actual_transactions(id),
    class               TEXT NOT NULL CHECK(class IN (
                            'fixed_recurrent', 'variable_recurrent',
                            'irregular_recurrent', 'exceptional', 'transfer',
                            'income', 'refund', 'unknown'
                        )),
    source              TEXT NOT NULL CHECK(source IN (
                            'user_confirmed', 'deterministic', 'ai_proposal'
                        )),
    confidence_ppm      INTEGER NOT NULL CHECK(confidence_ppm BETWEEN 0 AND 1000000),
    note                TEXT,
    updated_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS inference_proposals (
    id                  INTEGER PRIMARY KEY,
    proposal_type       TEXT NOT NULL,
    payload_json        TEXT NOT NULL,
    confidence_ppm      INTEGER NOT NULL CHECK(confidence_ppm BETWEEN 0 AND 1000000),
    effect_days         INTEGER,
    status              TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'accepted', 'rejected')),
    created_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at         TEXT
);

CREATE TABLE IF NOT EXISTS scenarios (
    id                      INTEGER PRIMARY KEY,
    name                    TEXT NOT NULL UNIQUE,
    currency                TEXT NOT NULL CHECK(length(currency) = 3),
    reserve_minor           INTEGER NOT NULL DEFAULT 0,
    explicit_assets_minor   INTEGER,
    assets_as_of            TEXT,
    max_horizon_days        INTEGER NOT NULL DEFAULT 36525,
    updated_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK((explicit_assets_minor IS NULL) = (assets_as_of IS NULL))
);

CREATE TABLE IF NOT EXISTS forecast_rules (
    id                  INTEGER PRIMARY KEY,
    scenario_id         INTEGER NOT NULL REFERENCES scenarios(id),
    label               TEXT NOT NULL,
    direction           TEXT NOT NULL CHECK(direction IN ('income', 'expense')),
    amount_minor        INTEGER NOT NULL CHECK(amount_minor > 0),
    currency            TEXT NOT NULL CHECK(length(currency) = 3),
    cadence             TEXT NOT NULL CHECK(cadence IN ('once', 'monthly')),
    day_of_month        INTEGER CHECK(day_of_month BETWEEN 1 AND 31),
    starts_on           TEXT NOT NULL,
    ends_on             TEXT,
    source              TEXT NOT NULL CHECK(source IN ('user_confirmed', 'deterministic_estimate')),
    confidence_ppm      INTEGER NOT NULL CHECK(confidence_ppm BETWEEN 0 AND 1000000),
    evidence_json       TEXT NOT NULL DEFAULT '[]',
    created_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK((cadence = 'monthly') = (day_of_month IS NOT NULL)),
    CHECK(ends_on IS NULL OR ends_on >= starts_on)
);

CREATE INDEX IF NOT EXISTS forecast_rules_by_scenario
    ON forecast_rules(scenario_id, starts_on);

CREATE TABLE IF NOT EXISTS runway_snapshots (
    id                              INTEGER PRIMARY KEY,
    scenario_id                     INTEGER NOT NULL REFERENCES scenarios(id),
    calculated_at                   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    as_of                           TEXT NOT NULL,
    last_actual_data                TEXT NOT NULL,
    currency                        TEXT NOT NULL,
    runway_days                     INTEGER,
    zero_date                       TEXT,
    liquid_assets_minor             INTEGER NOT NULL,
    reserve_minor                   INTEGER NOT NULL,
    projected_balance_minor         INTEGER NOT NULL,
    historical_expense_applied_minor INTEGER NOT NULL,
    explanation_json                TEXT NOT NULL
);
