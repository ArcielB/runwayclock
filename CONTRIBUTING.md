# Contributing

RunwayClock welcomes contributions. V0 is still changing quickly, so open an
issue before investing in a large architectural change.

## Financial-data rule

Never commit a real bank statement, SQLite database, widget snapshot, account
identifier, import screenshot, or log containing transaction descriptions.
Fixtures must be obviously synthetic and should use fictional IDs and merchants.

Before opening a pull request, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd app
npm ci
npm run check
npm run build
```

Linux contributors need the packages installed by
`scripts/install-linux-dev-deps.sh` for the full Tauri checks.

## Architectural rules

- Keep financial arithmetic in Rust and in integer minor units.
- Treat Pop!_OS as the primary test environment, not as the Linux architecture;
  preserve broad distro, desktop, and version compatibility.
- Keep actual transactions, interpretations, forecast facts, and projections
  separate.
- Preserve imported raw evidence.
- Treat inference and AI output as proposals, never silent mutations.
- Never overwrite `user_confirmed` interpretations.
- Keep the GNOME extension database-blind.
- Add a synthetic regression test for every new bank-format edge case.

## Pull requests

Describe the user-visible behavior, model assumptions, migration impact, and test
evidence. Changes that can alter a zero-date should also update the explanation
contract or document why it is unchanged.
