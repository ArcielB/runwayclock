PRAGMA foreign_keys = OFF;

BEGIN IMMEDIATE;

CREATE TABLE import_batches_v2 (
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

INSERT INTO import_batches_v2(
    id, account_id, profile_id, source_name, content_sha256, profile_sha256,
    profile_config_toml,
    imported_at, row_count, inserted_count, duplicate_count, error_count
)
SELECT
    b.id, b.account_id, b.profile_id, b.source_name, b.content_sha256,
    'legacy-profile-' || profile_id,
    p.config_toml,
    b.imported_at, b.row_count, b.inserted_count, b.duplicate_count, b.error_count
FROM import_batches b
JOIN import_profiles p ON p.id = b.profile_id;

DROP TABLE import_batches;
ALTER TABLE import_batches_v2 RENAME TO import_batches;

PRAGMA user_version = 2;

COMMIT;

PRAGMA foreign_keys = ON;
