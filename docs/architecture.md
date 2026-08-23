# Architecture boundaries

## Dependency direction

```text
runway-core
    ↑
runway-db ← runway-import
    ↑            ↑
    ├──── runway-cli
    └──── Tauri command bridge ← Svelte desktop interface
                         │
                         └──── sanitized widget.json ──── GNOME extension
```

`runway-core` is platform-independent. It accepts dated facts and a historical
burn summary; it never opens a database or reads a statement. The Tauri desktop
application and CLI already call the same API; Android and later clients can do
the same.

## Actual and forecast separation

SQLite is authoritative. Its main conceptual groups are:

| Boundary | Tables | Meaning |
|---|---|---|
| Raw | `import_batches`, `raw_import_rows` | Original decoded cells and row-level parse errors |
| Normalized actual | `accounts`, `actual_transactions` | Dates, signed minor-unit amounts, descriptions, balances |
| Interpretation | `transaction_interpretations`, `inference_proposals` | Persistent user truth and non-authoritative hypotheses |
| Forecast | `scenarios`, `forecast_rules` | Reserve, asset facts, allowed future flows |
| Result | `runway_snapshots` | Reproducible calculation explanation |

Forecast rows never enter `actual_transactions`. Corrections do not overwrite raw
descriptions or imported amounts.

## Import identity and reconciliation

Each source file and normalized profile configuration are SHA-256 hashed per
account. Reimporting identical bytes with the same mapping returns the existing
batch and inserts zero rows. If a user corrects a mapping after row errors, the
same file receives a new parsing pass; transaction reconciliation prevents the
already-successful rows from being duplicated.

Within overlapping statements, the transaction identity is:

1. bank transaction ID plus owned account, when supplied;
2. otherwise account, date, signed amount, normalized description, and balance
   after the transaction when available.

The fallback is intentionally inspectable but cannot perfectly distinguish two
identical same-day transactions when neither ID nor balance is present. A future
reconciliation proposal should preserve ambiguity instead of silently dropping
evidence.

## User corrections

`transaction_interpretations` has one current record per actual transaction.
Desktop and CLI corrections are stored as `user_confirmed` at confidence 1.0 and
survive every import and recalculation. Future inference must not overwrite them.

## Widget isolation

The GNOME extension knows only:

```text
scenario
calculated_at
as_of
runway_days
zero_date
display_duration
last_actual_data
change_30d
confidence
```

It does not receive balances, transaction descriptions, amounts, evidence, bank
names, or the SQLite path. The main process writes the file atomically after a
successful calculation.

## Desktop application boundary

The Tauri command layer remains thin:

```text
Svelte import mapper / fact editor / review / explanation UI
                         ↓ typed Tauri commands
Rust import + db + core crates
```

TypeScript formats values for display and coordinates screens; it performs no
financial calculation. Statement preview, parsing, reconciliation, facts,
corrections, calculation, persistence, and widget publication all cross typed
native commands into Rust.

Both the desktop app and CLI use the same Linux paths:

```text
~/.local/share/runwayclock/runwayclock.db
~/.local/share/runwayclock/widget.json
```

## Public distribution

Pull requests run Rust formatting, linting, tests, Svelte checking, and a frontend
build in CI. Version tags build draft Linux AppImage and Debian releases. Signed
automatic application updates require a stable public repository URL and signing
key, so they remain intentionally disabled rather than shipping an unverifiable
update channel.
