# Get started with Hours

## Install on macOS

1. Download the Apple Silicon Mac ZIP from the [latest release](https://github.com/chrisvo/hours-rs/releases/latest).
2. Open the ZIP and move **Hours.app** to your Applications folder.
3. Open **Hours**.

The release is built for Apple Silicon Macs. The app has an ad-hoc signature and is not notarized by Apple. If macOS blocks it, verify that you downloaded it from this repository. Follow [Apple's instructions for opening an app from an unidentified developer](https://support.apple.com/en-us/102445), or [build from source](../CONTRIBUTING.md).

Release assets include SHA-256 checksums. In the directory containing the ZIP and checksum file, run the `shasum -a 256 -c` command shown in the release notes.

## Download your first offices

Connect to the internet before downloading. Hours starts without bundled prayer texts.

1. Choose the date in the top bar.
2. Open **Offline library** in the sidebar.
3. Leave **Offline mode** off while downloading.
4. Choose a number of days and select **Download all hours**.
5. Wait for each needed date to show **5/5 hours**.

You can also open a missing hour and select **Download this hour**. A stopped download retains complete offices. Retrying downloads only the missing offices.

The provider may refuse dates outside its available range. Hours shows an error instead of substituting a different date's prayers.

## Pray without a connection

Enable **Offline mode** in the library after downloading. Choose an hour from the sidebar and read its text. Search, notes, bookmarks, highlights, and prayer progress continue to work locally.

Begin with Morning Prayer or Evening Prayer if you are new to the office. Follow the instructions printed in the rubric color. Texts can include alternatives, which are not all intended to be prayed together.

The **Invitatory** introduces the day's prayer before Morning Prayer or the Office of Readings, whichever you pray first. Psalm 95 is the usual choice. Psalms 24, 67, and 100 are alternatives. Use one invitatory psalm, not all four. See the [General Instruction, paragraphs 34–35](https://www.catholicculture.org/culture/library/view.cfm?recnum=5138).

The Daytime Prayer document contains the source's midmorning, midday, and midafternoon options. Follow the rubrics for the hour you choose.

## Keep your place and reflections

Hours remembers your position in each downloaded office. Right-click a passage to bookmark it, highlight it, copy it, or write a note. Double-click also opens a note.

Select **Save note** when you finish. Closing the note also saves it. The journal button opens notes and bookmarks from all dates. Mark an hour as prayed with **⌘Enter**, or use the button at the end of its text.

## Back up your library

1. Open **Offline library**.
2. Expand **Back up or move your library**.
3. Enter an absolute file path, such as `/Users/your-name/Desktop/prayers.hours.json`.
4. Select **Export backup**.

Choose a new filename for each export. Hours refuses to overwrite an existing backup.

To restore a backup, enter its path and select **Import backup**, or drop the file onto the app. Imports add missing offices, notes, bookmarks, highlights, and prayer marks. They do not replace existing records. Reading positions and appearance settings are local to the device and are not included in backups.

A backup contains personal notes and downloaded prayer texts. Keep it private and respect the texts' copyrights.

On macOS, the database is `~/Library/Application Support/com.chrisvo.hours-rs/hours.sqlite3`. The `HOURS_RS_HOME` environment variable selects another directory for testing or a separate library.
