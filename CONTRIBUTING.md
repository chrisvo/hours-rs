# Contribute to Hours

Hours is a small native Rust app for prayer. Contributions should keep reading reliable, preserve personal notes, and make the interface easier to use.

## Set up a development build

Install Rust 1.88 or later with rustup. Current stable Rust is used in CI. On macOS, install the Xcode Command Line Tools with `xcode-select --install` if they are missing.

Fork the repository, clone your fork, and create a branch. From the project directory, run:

```sh
cargo run --locked
```

macOS on Apple Silicon is the verified target. Reports and fixes for other platforms are welcome, but test them before describing a platform as supported.

To keep development separate from your personal library, set `HOURS_RS_HOME` to a temporary directory:

```sh
HOURS_RS_HOME="$(mktemp -d)" cargo run --locked
```

## Check a change

Run the same checks as CI:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Use synthetic texts in tests. Do not commit downloaded offices, private notes, library backups, or prayer texts without permission to redistribute them. The deterministic test suite does not contact the live provider.

For a UI change, run the native app and check a compact window as well as the normal size. Check light and dark themes. Explain any platform or interaction that you could not verify.

For changes to downloads or storage, check date validation, partial-download behavior, restart persistence, and whether existing annotations survive. Read the [offline library design](docs/design/offline-library.md) first.

## Find the relevant code

| Path | Responsibility |
| --- | --- |
| `src/app.rs` | Native interface, navigation, notes, and download coordination |
| `src/model.rs` | Hours, offices, sections, paragraphs, and text styles |
| `src/provider.rs` | iBreviary sessions, metadata checks, and HTML parsing |
| `src/store.rs` | SQLite persistence and library backups |
| `src/theme.rs` | Colors, fonts, and reading styles |
| `src/main.rs` | Desktop startup and command-line commands |
| `tests/fixtures/` | Synthetic provider documents |

## Build and verify the Mac app

The bundle script needs Python 3, Rust's license documents, and cargo-about to collect dependency notices. Install the Rust components once:

```sh
rustup component add rust-docs
cargo install --locked --version 0.9.2 --features cli cargo-about
```

```sh
scripts/bundle-macos.sh
open dist/Hours.app
```

Use `--debug` for a development bundle. The script includes licensing notices and creates a ZIP archive. It does not notarize the app.

If your personal library already contains a complete date, run the network-denied smoke check:

```sh
scripts/verify-offline.sh YYYY-MM-DD
```

This check exports your library to a temporary directory, restores a separate copy, compares the selected day's five offices, and captures the native app. Its artifacts contain personal data. Keep them local.

## Open a pull request

Keep a pull request focused on one problem. Describe what changed, why it matters, and how you checked it. Add a regression test for a bug when the behavior can be tested meaningfully.

For a large feature, open an issue first so contributors can discuss scope. Avoid changes to liturgical content without a cited source. Do not remove source credits or replace a missing office with text from another date.

By contributing, you agree that your original code contribution is available under the repository's MIT license. See the [community guidelines](CODE_OF_CONDUCT.md).
