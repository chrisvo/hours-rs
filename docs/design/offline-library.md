# An offline prayer library

Hours reads a local office by civil date and hour. Startup, navigation, search, notes, fonts, themes, and prayer progress do not create network requests. A separate download action acquires complete offices. A missing hour stays visibly missing.

The source is iBreviary's modern Roman English office. Texts are personal downloads, not part of the repository or application bundle. Offline coverage is finite and visible per date. Hours does not calculate the Roman calendar or compose offices from a bundled corpus.

## Documents and personal data

An `Office` contains a date, hour, celebration, source URL, and ordered sections of styled paragraphs. `Library` owns the SQLite connection. A download worker owns an HTTP client and cookie jar and returns complete offices over a channel. The main thread commits those documents. Network requests do not hold database transactions.

`Library::office(date, hour)` returns an optional local document. `Library::save_office` inserts without replacing an existing document. Availability comes from committed documents. Retrying a download skips complete offices and requests the missing ones.

Annotations are separate rows with the date, hour, paragraph ID, and original quote. A note draft retains its own passage identity independently of the reader selection. Saving a note preserves the latest stored bookmark and highlight state. There is no content refresh or deletion operation that can silently detach a note from its original text.

## Date verification

The source ignores date query parameters on its prayer URL. Each requested date therefore gets an isolated session. The provider posts the date and English language, reads the returned calendar date, fetches each hour, and verifies the calendar again before accepting the content. Missing or mismatched dates, languages, and hour labels fail closed.

The parser preserves ordered sections, rubrics, line breaks, alternatives, credits, and local anchor links. Downloaded documents contain no executable HTML. Links in the stored content point only to existing local anchors.

The provider uses bounded connection and request timeouts. Cancellation prevents subsequent requests and commits. An active HTTP request may take until its timeout to finish.

## Backups

Imports validate the version, profile, content limits, dates, and links before a transaction commits the pack. Invalid data cannot partially import. Existing offices and annotations remain unchanged. Export writes a complete temporary file and publishes it without overwriting an existing destination.

Backups contain offices, annotations, and prayer completion. Appearance settings and reading positions remain device-local. Refreshing the currently displayed office after import preserves its live position. Importing a previously missing office starts at that office's own position.

## Verification

The deterministic suite uses synthetic texts to exercise parser rejection, formatting, concurrent inserts, transactional rollback, immutable documents, and persistence after reopening. App tests drive egui keyboard and pointer events, including compact-window controls, note identity, and both backup import entry points.

`scripts/verify-offline.sh YYYY-MM-DD` runs the built Mac app with operating-system network access denied. It restores a backup into a separate temporary library, compares the selected day's five offices, and captures the native window. It requires an already-downloaded date and keeps its personal-data artifacts local.

Live source checks remain separate from the deterministic suite because upstream availability and markup can change.
