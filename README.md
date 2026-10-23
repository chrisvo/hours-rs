<img src="assets/icon.svg" width="64" height="64" alt="Hours icon">

# Hours

A native Rust desktop reader for the modern Roman Catholic Liturgy of the Hours in English. Inspired by [Rasmic's bible-rs](https://github.com/michaelshimeles/bible-rs).

Cinzel headings recall Roman inscriptions. Literata sets the prayer text. Both fonts are bundled. Light, parchment, and dark themes work without a network connection.

## Run the app

Requires Rust 1.88 or later. macOS is the verified desktop target.

```sh
cargo run --release
```

Choose an hour, then select **Download this hour**. To prepare several days, open **Offline library**, choose a date range, and select **Download all hours**. Each completed office stays on your device. Interrupted downloads retain complete offices; retry skips them.

The app includes the Office of Readings, Morning Prayer, Daytime Prayer, Evening Prayer, and Night Prayer. The Daytime Prayer document contains the midmorning, midday, and midafternoon options supplied by iBreviary. Follow its rubrics to choose the appropriate texts.

## Pray offline

Download the dates you need before disconnecting. Turn on **Offline mode** in the library to disable downloads. Reading, search, notes, bookmarks, highlights, reading position, and prayer progress use local storage. The app makes no network requests during startup or navigation.

Offline coverage is limited to downloaded dates. A missing office is shown as not downloaded. Hours does not calculate future liturgical calendars or substitute prayers from another date. iBreviary controls the available dates and its calendar's observances.

Prayer texts are personal downloads from [iBreviary](https://www.ibreviary.org/en/). They are not distributed in this repository or the application bundle. Source credits and rubrics remain in each office. Hours is an independent reader and is not an official iBreviary app.

## Keep notes and bookmarks

Right-click a passage to bookmark it, highlight it, copy it, or write a note. Double-click a passage to open its note. Select **Save note** to persist your reflection. Closing the note also saves it. The journal lists saved passages across dates.

The app stores data in a local SQLite file. On macOS the directory is `~/Library/Application Support/com.chrisvo.hours-rs/`. Set `HOURS_RS_HOME` to use another directory.

Open **Offline library → Back up or move your library** to export a JSON backup. The backup includes downloaded offices, notes, bookmarks, highlights, and prayer marks. Import it on another device or drop it onto the app. Imports add missing records without replacing existing documents or personal notes.

## Keyboard controls

| Key | Action |
| --- | --- |
| ⌘1 through ⌘5 | Choose an hour |
| Left / Right | Previous / next hour |
| ⌘K | Choose a date |
| ⌘F | Search the current hour |
| ⌘J | Open notes and bookmarks |
| ⌘B | Toggle the hours sidebar |
| ⌘Enter | Mark the current hour as prayed |
| ⌘+ / ⌘− / ⌘0 | Increase, decrease, or reset text size |
| ⌘, | Open reading appearance |
| Escape | Close panels and leave focus mode |

On Windows and Linux, use Ctrl in place of ⌘. Those platforms have not yet been verified.

## Use the command line

```sh
cargo run -- download 2026-10-22 7
cargo run -- list
cargo run -- read 2026-10-22 morning
cargo run -- --offline --date 2026-10-22 --hour morning
cargo run -- export ~/Desktop/prayers.hours.json
cargo run -- import ~/Desktop/prayers.hours.json
```

`download` accepts 1 to 30 days. The other commands use local data only. The source can refuse dates outside its available range.

## Build a Mac application

```sh
scripts/bundle-macos.sh
open dist/Hours.app
```

The script creates an ad-hoc signed local app with embedded fonts and a `dist/Hours-macOS.zip` archive. It does not notarize a public release. Use `--debug` for a development bundle.

## Verify changes

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Unit tests use synthetic prayers, including parser and persistence tests. Live-source checks are separate from the deterministic suite.

Capture the actual native window for a selected local office:

```sh
cargo run -- --offline --date 2026-10-22 --hour evening --screenshot /tmp/hours.png
```

The [offline-library design](docs/design/offline-library.md) describes ownership and download verification. The [decision log](docs/decisions.tsv) records the implementation choices.

On macOS, verify a downloaded date against the built app with `scripts/verify-offline.sh YYYY-MM-DD`. It blocks network access, compares all five offices after a backup restore into a temporary library, and captures the native window. The temporary directory contains personal prayer texts and stays on your machine.

## Credits

[bible-rs](https://github.com/michaelshimeles/bible-rs) informed the native reader design. Hours uses egui and eframe, SQLite, and reqwest. This is a separate implementation.

Cinzel, Literata, and Inter are distributed under the SIL Open Font License. Their license files are in [assets/fonts](assets/fonts). Prayer texts retain their publishers' copyrights. Application code is MIT licensed.
