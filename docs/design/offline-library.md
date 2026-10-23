# An offline prayer library

The app opens a local office by civil date and hour. Navigation, search, notes, fonts, themes, and prayer progress never create network requests. A separate Download action acquires complete offices. A missing hour stays visibly missing.

The current source is iBreviary's modern Roman English office. Texts are personal downloads, not part of the repository or application bundle. Offline coverage is finite and shown per date. This release does not calculate the Roman calendar or compose propers from a bundled corpus.

## Data and ownership

`Office` contains a date, hour, celebration, source URL, and ordered sections of styled paragraphs. `Library` owns the SQLite connection. A download worker owns its own HTTP client and cookie jar and returns complete offices over a channel. The main thread commits those documents. No network request holds a database transaction.

`Library::office(date, hour)` returns an optional local document. `Library::save_office` inserts without replacing an existing document. `Library::availability(date)` derives available hours from committed documents. A download retries missing items and skips complete items.

Personal annotations are separate rows with the office identity, paragraph ID, and original quote. They survive restarts and are never deleted by content operations. This release deliberately has no content refresh or deletion operation, so an annotation cannot silently attach to a changed edition.

## Date verification

The source ignores date query parameters on its prayer URL. Each date gets an isolated session. The provider posts the date and English language, reads the returned calendar date, fetches each hour, and verifies the calendar again before accepting content. Missing or mismatched dates and hour labels fail closed. The provider preserves rubrics, credits, and alternative texts.

## Synthesis

Four independent candidates favored a local office library over a new calendar engine. Candidate 4 is the base because one library owns writes and exposes local reading plus explicit downloads. The independent judge also selected candidate 4. The design takes candidate 3's single writer and candidate 1's explicit incomplete-date state. It rejects a second personal-data database because one writer can commit both kinds of local data without coupling their lifecycle. It postpones content revisions and persistent job machinery because complete documents are immutable and retry derives work from existing documents.

The sketches lacked distinct viable recommendations; each compared the local library with a calendar engine and rejected the engine for the absent verified corpus. Their convergence supports the simpler local library, but does not prove the provider correct.

## Verification plan

Test parser formatting and date rejection with synthetic fixtures. Test persistence by closing and reopening a temporary database. Exercise live source downloads separately. Run the installed desktop app with networking disabled, read multiple downloaded hours, search, save a note and bookmark, and restart. Inspect screenshots at normal and compact window sizes. Build a macOS bundle with embedded fonts and no remote assets.

## Verification results

The deterministic suite passes 32 tests. It covers provider metadata and formatting, transactional import rollback, immutable offices, notes and bookmarks after reopening, native egui keyboard navigation, visible and clickable library controls at 760 by 560, and preserved scroll position after both import entry points, including newly imported offices. A note draft stores its original passage identity independently of the reader's current selection.

Live personal downloads verified all five offices for October 22 through 28, 2026. `scripts/verify-offline.sh 2026-10-23` ran the Mac bundle with OS network access denied, restored all 35 offices into a separate library, compared the five selected offices byte for byte, and captured the rendered native window. The texts and screenshots remain local and are not committed.

Native GUI automation could inspect an earlier window but could not complete interactive actions because the computer-use server rejected its window state. Interaction coverage comes from actual egui input events in the deterministic tests. Native screenshots verify rendering separately. The independent code review found the compact sidebar and import scroll regressions, which now have regression tests. Independent reviewers used the same model family because the other configured providers were unavailable.

Model the Domain shaped the immutable document and separate annotation records, and the note draft's stable passage identity. Prove It Works shaped the restart tests and the repeatable network-denied bundle check. Separate Before Serializing Shared State gave each download session its own HTTP client and cookie jar. The main thread alone writes the library.
