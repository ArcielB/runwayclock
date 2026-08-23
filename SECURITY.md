# Security and financial-data handling

RunwayClock processes highly sensitive local financial data. Please use
[GitHub's private vulnerability report](https://github.com/ArcielB/runwayclock/security/advisories/new)
instead of opening a public issue. Do not attach real statements, databases, or
account information to any report.

The intended trust boundary is:

- statement files and SQLite data remain on the user's device;
- the application has no analytics, account, cloud, or bank-network connection;
- the GNOME extension reads only a sanitized runway snapshot;
- AI is not part of calculation correctness;
- imported raw evidence is never overwritten by normalization or interpretation;
- user-confirmed interpretations take precedence over later inference.

Before every public push, maintainers must verify that the pending history
contains no real statements, databases, widget snapshots, account identifiers,
or screenshots containing personal financial information.
