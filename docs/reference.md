# Hours reference

## Keyboard controls

| Key on macOS | Action |
| --- | --- |
| ⌘1 through ⌘5 | Choose an hour |
| Left or Right | Previous or next hour |
| ⌘K | Choose a date |
| ⌘F | Search the current hour |
| ⌘J | Open notes and bookmarks |
| ⌘B | Toggle the hours sidebar |
| ⌘Enter | Mark the current hour as prayed |
| ⌘+ / ⌘− / ⌘0 | Increase, decrease, or reset text size |
| ⌘, | Open reading appearance |
| Escape | Close panels and leave focus mode |

Windows and Linux use Ctrl in place of ⌘. Those platforms are not yet verified.

## Command line

These examples run from a source checkout. The release app's executable is also available at `Hours.app/Contents/MacOS/hours-rs`.

```sh
cargo run --release --locked -- --help
cargo run --release --locked -- download today 7
cargo run --release --locked -- list
cargo run --release --locked -- read today morning
cargo run --release --locked -- --offline --date today --hour morning
cargo run --release --locked -- export ~/Desktop/prayers.hours.json
cargo run --release --locked -- import ~/Desktop/prayers.hours.json
```

| Argument | Values |
| --- | --- |
| Date | `today` or `YYYY-MM-DD`, within years 0001 through 9999 |
| Hour | `readings`, `morning`, `daytime`, `evening`, `night` |
| Download length | 1 through 30 days, default 1 |
| `--offline` | Start the app with downloads disabled |
| `--screenshot PATH` | Capture the native window as a PNG and exit |
| `HOURS_RS_HOME` | Override the local data directory |

The `download` command explicitly requests a network connection. `read`, `list`, `import`, and `export` use local data. Download availability depends on the provider.

## Storage and backups

The SQLite library stores immutable office documents, separate personal annotations, prayer completion, reading positions, and appearance settings.

Backups use a versioned JSON format and have a 64 MiB import limit. They include offices, annotations, and prayer completion. They exclude appearance settings and reading positions. Import validates the whole backup before committing it and never overwrites existing documents or annotations. Export refuses an existing destination.

## Appearance

The app includes light, parchment, and dark themes. Reading controls offer text sizes from 15 to 30 points, line spacing from 1.25 to 2.0, and a preferred text width from 460 to 850 pixels. The reader adapts to the space available.

Cinzel is used for headings, Literata for prayer text, and Inter for small interface labels. Fonts are bundled, so appearance does not require a connection.
