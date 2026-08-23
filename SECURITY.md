# Security and financial-data handling

RunwayClock processes highly sensitive local financial data. Please report
security vulnerabilities privately to the repository maintainers rather than in
a public issue. A dedicated security contact will be added before the first
public release.

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
