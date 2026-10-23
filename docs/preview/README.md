# Documentation screenshots

The README screenshots come from the actual native Hours app. They use a temporary library containing public-domain text, not a personal prayer library or a downloaded modern office.

`psalm.txt` contains Psalm 22 in the Douay-Rheims Challoner numbering, also known as Psalm 23. The text is from the [public-domain Douay-Rheims Bible](https://www.gutenberg.org/cache/epub/8321/pg8321-images.html). Line breaks were adjusted for the screenshot. This fixture is not a complete liturgical office and is not bundled with the app.

The caption inside the app identifies it as a preview. In normal use, Hours displays the modern English office downloaded from iBreviary with the source's rubrics and credits.

Regenerate the screenshots on a Mac with a graphical session:

```sh
python3 scripts/capture-previews.py /absolute/path/to/hours-rs
```

The script creates a separate temporary database, renders parchment and dark themes, and writes PNG files under `docs/images/`. It does not read your personal library or download any prayer texts.
