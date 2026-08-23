# RunwayClock

RunwayClock answers one question:

> How long can I continue living without needing to earn additional money?

It is a local-first desktop application that reconstructs expected cash flows
from bank statement history plus facts the user explicitly confirms.

```text
bank CSV → raw evidence + normalized ledger → explainable forecast
                                                   ↓
                              RUNWAY · 14 months 12 days
```

RunwayClock is not a budgeting system. It does not ask users to maintain category
limits or classify an entire ledger.

## Current V0

The repository now contains an installable Tauri 2 + Svelte desktop interface,
a platform-independent Rust calculation engine, SQLite storage, CSV ingestion,
and a GNOME top-panel widget.

The interface provides:

- first-run CSV onboarding with preview and editable column mapping;
- automatic Turkish/English header, delimiter, encoding, number, and date suggestions;
- saved bank profiles and owned-account context for later imports;
- explicit new/already-known/error counts after every reconciliation;
- a persistent dashboard with runway, freshness, reserve, and explanation;
- a high-impact transaction review queue rather than mandatory categorization;
- forms for reserve, liquid assets, scholarships, recurring income, and known expenses;
- English and Turkish UI copy;
- automatic sanitized widget snapshot publication after calculation.

There is no cloud account, analytics, bank API, or required AI.

## Install on Linux

The recommended installer works without root access on x86_64 and ARM64 Linux.
It downloads the latest AppImage, verifies the release checksums, extracts it so
FUSE is not required, adds RunwayClock to the application menu, and installs the
GNOME widget when GNOME is detected.

```bash
curl -fsSLO https://github.com/ArcielB/runwayclock/releases/latest/download/runwayclock-installer.sh
bash runwayclock-installer.sh
```

Then open **RunwayClock** from the application menu. On a new GNOME widget
installation, log out and back in once so GNOME Shell discovers the extension.

Update to the newest release without changing financial data:

```bash
runwayclock-update
```

Uninstalling preserves the local database by default:

```bash
runwayclock-uninstall
```

To intentionally remove both the product and all RunwayClock financial data:

```bash
runwayclock-uninstall --purge-data
```

The [GitHub Releases](https://github.com/ArcielB/runwayclock/releases) page also
provides AppImage, Debian, and RPM packages plus `SHA256SUMS` for manual
installation. See [Linux support](docs/linux-support.md) for compatibility and
packaging details.

## The normal user workflow

### First statement

1. Launch RunwayClock.
2. Choose **Import your first statement**.
3. Select a CSV exported by the bank.
4. Confirm the detected mapping and owned account.
5. Import, set a reserve, and add facts history cannot know.
6. Read the runway and explanation.

### Every later statement

1. Choose **Import & update**.
2. Select the new CSV without trimming overlapping dates.
3. RunwayClock recognizes the saved format/account and reconciles the overlap.
4. Only genuinely new transactions are inserted; the runway and widget update.

Importing identical bytes twice inserts zero transactions. When bank transaction
IDs are absent, reconciliation uses account, date, signed amount, normalized
description, and post-transaction balance when available.

See [the full user workflow](docs/user-workflow.md).

## Calculation policy

V0 intentionally uses a simple, inspectable baseline:

1. Liquid assets come from the latest balance of each owned account in the
   scenario currency, unless the user provides an explicit asset total and date.
2. Historical debits count as spending by default, so irregular costs are not
   silently discarded.
3. User-confirmed transfers and truly exceptional costs are excluded. Confirmed
   refunds reduce observed spending.
4. Included spending is spread across the exact elapsed observation period using
   rational integer arithmetic.
5. Only explicit future facts are forecast. Historical salary never becomes
   assumed no-work income.
6. Runway ends on the first future date whose closing balance is at or below the
   reserve.

Actual transactions, interpretations, forecast facts, and calculated snapshots
remain separate. Read [the forecast policy](docs/forecast-v0.md) and
[architecture boundaries](docs/architecture.md) for detail.

## Build for development on Linux

Prerequisites:

- Node.js 24+
- stable Rust
- Tauri's Linux WebKit/GTK development libraries

On Ubuntu or Pop!_OS, install the system libraries:

```bash
./scripts/install-linux-dev-deps.sh
```

Install dependencies and launch the development build:

```bash
cd app
npm ci
npm run tauri dev
```

The development database is the same one used by installed builds:

```text
~/.local/share/runwayclock/runwayclock.db
```

Real statements and databases belong outside the repository. `private/`, common
database extensions, and `*.bank.csv` are ignored.

## Verify

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd app
npm ci
npm run check
npm run build
```

The CI workflow installs Linux dependencies and runs the full set. Release tags
build and publish checksum-protected x86_64 and ARM64 AppImage, Debian, and RPM
packages on an Ubuntu 22.04 compatibility baseline.

## GNOME indicator

RunwayClock writes a sanitized snapshot to:

```text
~/.local/share/runwayclock/widget.json
```

The extension never opens SQLite and receives no balances or transaction text.
Install it on GNOME 40–50 with:

```bash
./widgets/gnome/install.sh
```

It displays a live countdown such as `RUNWAY 14m 12d` and actual-data freshness.
Left-click the panel number to open the full dashboard, where statements and
future facts can be added or corrected. Right-click exposes the compact status
menu, which also includes an **Open RunwayClock** action.

## Repository layout

```text
app/                     Tauri 2 + Svelte desktop product
  src/                   onboarding, dashboard, review, and facts UI
  src-tauri/             native command bridge and packaging
crates/runway-core/      platform-independent money and runway simulation
crates/runway-db/        SQLite schema, corrections, facts, and snapshots
crates/runway-import/    preview, parsing, raw evidence, and reconciliation
crates/runway-cli/       headless diagnostic and automation interface
widgets/gnome/           database-blind GNOME indicator
tests/fixtures/          synthetic financial evidence only
```

## Still intentionally incomplete

- fixed/variable/irregular spending decomposition beyond the V0 baseline;
- transfer-pair and credit-card-payment proposals;
- FX conversion inside one scenario;
- signed automatic software updates;
- Android notification ingestion;
- final public license and security contact.

The Cargo crates remain `publish = false`. No real financial fixture should ever
be committed.
