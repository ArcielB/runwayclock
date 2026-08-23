# User workflow

RunwayClock is designed around occasional statement updates, not continuous
budget maintenance.

## First run

1. Open RunwayClock and choose **Import your first statement**.
2. Select a bank CSV exported from online banking.
3. RunwayClock previews the file without writing data. It detects the likely
   encoding, delimiter, preamble, date, description, amount, balance, and
   transaction-ID columns.
4. Confirm the suggested mapping and give the owned account a stable name.
5. Import. Raw rows are preserved and normalized transactions enter SQLite.
6. Set the reserve and, only if imported balances are incomplete, enter total
   liquid assets with an as-of date.
7. Add facts history cannot know, such as scholarship end dates and known future
   expenses.
8. Read the runway and its explanation.

## Updating reality

When a later statement is available:

1. Choose **Import & update** and select the new CSV.
2. If its headers match a saved profile, RunwayClock restores the mapping,
   account, and currency automatically. Confirm the account if multiple owned
   accounts share the same bank format.
3. Import. Exact-file hashes, bank transaction IDs, and fallback fingerprints
   reconcile overlap. The result explicitly separates new, already-known, and
   failed rows.
4. The dashboard recalculates and publishes a new sanitized widget snapshot.

Importing the same file twice must insert zero transactions. Overlapping date
ranges are expected and do not require a user to trim CSV files.

## Correcting important misunderstandings

The **Review** screen presents the largest unresolved outflows first and estimates
their rough effect in days. A user can mark an item as:

- a transfer between owned accounts;
- truly exceptional and not representative of future spending;
- ongoing spending that should remain in the baseline.

The correction is stored separately from the imported transaction and reused on
every future calculation. Users are not asked to categorize the whole ledger.

## Maintaining future facts

The **Future facts** screen owns forecast truth. It supports one-time and monthly
income or expenses, optional end dates, reserve, and explicit asset totals.
These records never become fake actual transactions.

Deleting or changing a forecast fact causes a new calculation; it does not alter
bank evidence.

## Software installation and upgrades

Linux release tags build an AppImage and Debian package in GitHub Actions. The
first public release should provide both on the Releases page. Package upgrades
reuse the same SQLite database and run migrations on startup.

Signed automatic application updates are intentionally deferred until the public
repository URL and signing key are finalized. Financial-data updates already use
the in-app import workflow above.
