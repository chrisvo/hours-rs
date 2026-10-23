#!/usr/bin/env python3
"""Capture the native reader with a separate public-domain documentation fixture."""

import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent.parent
binary = Path(sys.argv[1]).resolve()
output = root / "docs" / "images"
output.mkdir(parents=True, exist_ok=True)
stanzas = (root / "docs" / "preview" / "psalm.txt").read_text().strip().split("\n\n")
date = "2026-10-23"

with tempfile.TemporaryDirectory(prefix="hours-documentation-") as directory:
    environment = dict(os.environ, HOURS_RS_HOME=directory)
    subprocess.run([str(binary), "list"], env=environment, check=True)
    database = Path(directory) / "hours.sqlite3"
    document = {
        "date": date,
        "hour": "evening",
        "celebration": "Interface preview · public-domain psalm",
        "source_url": "https://www.gutenberg.org/cache/epub/8321/pg8321-images.html",
        "sections": [{
            "title": "PSALM 23 · DOMINUS REGIT ME",
            "blocks": [{
                "id": f"preview-{index}",
                "anchors": [],
                "runs": [{"text": text, "kind": "text"}],
            } for index, text in enumerate(stanzas)],
        }],
    }
    with sqlite3.connect(database) as connection:
        connection.execute("INSERT INTO offices VALUES (?, ?, ?)", (date, "evening", json.dumps(document)))
    for theme in ("Parchment", "Dark"):
        with sqlite3.connect(database) as connection:
            connection.execute(
                "INSERT OR REPLACE INTO settings VALUES (?, ?)",
                ("preferences", json.dumps({"theme": theme, "offline": True})),
            )
        subprocess.run([
            str(binary), "--offline", "--date", date, "--hour", "evening",
            "--screenshot", str(output / f"reader-{theme.lower()}.png"),
        ], env=environment, check=True, timeout=60)
