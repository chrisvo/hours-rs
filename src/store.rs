use crate::model::{Hour, Office, RunKind, validate_date};
use anyhow::{Context, Result, bail, ensure};
use chrono::NaiveDate;
use directories::ProjectDirs;
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const VERSION: u32 = 1;
const SOURCE_PROFILE: &str = "ibreviary-modern-roman-english";
const MAX_PACK_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub date: NaiveDate,
    pub hour: Hour,
    pub block_id: String,
    pub quote: String,
    pub note: String,
    pub bookmarked: bool,
    pub highlighted: bool,
}

impl Annotation {
    fn is_empty(&self) -> bool {
        self.note.trim().is_empty() && !self.bookmarked && !self.highlighted
    }
}

pub struct Library {
    connection: Connection,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pack {
    version: u32,
    source_profile: String,
    offices: Vec<Office>,
    annotations: Vec<Annotation>,
    completed: Vec<Completion>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion {
    date: NaiveDate,
    hour: Hour,
}

impl Library {
    pub fn open(path: &Path) -> Result<Self> {
        fs::create_dir_all(parent_directory(path))
            .with_context(|| format!("Creating library directory for {}", path.display()))?;
        let mut connection = Connection::open(path)
            .with_context(|| format!("Opening library {}", path.display()))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: u32 = transaction.pragma_query_value(None, "user_version", |r| r.get(0))?;
        ensure!(
            version <= VERSION,
            "Unsupported library schema version {version}"
        );
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS offices (
                date TEXT NOT NULL,
                hour TEXT NOT NULL,
                document TEXT NOT NULL,
                PRIMARY KEY (date, hour)
            ) WITHOUT ROWID;
            CREATE TABLE IF NOT EXISTS annotations (
                date TEXT NOT NULL,
                hour TEXT NOT NULL,
                block_id TEXT NOT NULL,
                quote TEXT NOT NULL,
                note TEXT NOT NULL,
                bookmarked INTEGER NOT NULL CHECK (bookmarked IN (0, 1)),
                highlighted INTEGER NOT NULL CHECK (highlighted IN (0, 1)),
                PRIMARY KEY (date, hour, block_id)
            ) WITHOUT ROWID;
            CREATE TABLE IF NOT EXISTS completed (
                date TEXT NOT NULL,
                hour TEXT NOT NULL,
                PRIMARY KEY (date, hour)
            ) WITHOUT ROWID;
            CREATE TABLE IF NOT EXISTS positions (
                date TEXT NOT NULL,
                hour TEXT NOT NULL,
                offset REAL NOT NULL CHECK (offset >= 0),
                PRIMARY KEY (date, hour)
            ) WITHOUT ROWID;
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            ) WITHOUT ROWID;",
        )?;
        transaction.pragma_update(None, "user_version", VERSION)?;
        transaction.commit()?;
        Ok(Self { connection })
    }

    pub fn default_path() -> Result<PathBuf> {
        if let Some(directory) = std::env::var_os("HOURS_RS_HOME") {
            ensure!(!directory.is_empty(), "HOURS_RS_HOME must name a directory");
            return Ok(PathBuf::from(directory).join("hours.sqlite3"));
        }
        let directories = ProjectDirs::from("com", "chrisvo", "hours-rs")
            .context("Finding the application data directory")?;
        Ok(directories.data_dir().join("hours.sqlite3"))
    }

    pub fn office(&self, date: NaiveDate, hour: Hour) -> Result<Option<Office>> {
        let document: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM offices WHERE date = ?1 AND hour = ?2",
                params![date.to_string(), hour.key()],
                |row| row.get(0),
            )
            .optional()?;
        document
            .map(|json| serde_json::from_str(&json).context("Reading a saved office"))
            .transpose()
    }

    pub fn save_office(&self, office: &Office) -> Result<bool> {
        validate_office(office)?;
        let document = serde_json::to_string(office)?;
        Ok(self.connection.execute(
            "INSERT INTO offices (date, hour, document) VALUES (?1, ?2, ?3)
             ON CONFLICT (date, hour) DO NOTHING",
            params![office.date.to_string(), office.hour.key(), document],
        )? == 1)
    }

    pub fn availability(&self, date: NaiveDate) -> Result<Vec<Hour>> {
        self.hours("SELECT hour FROM offices WHERE date = ?1", date)
    }

    pub fn downloaded_dates(&self) -> Result<Vec<(NaiveDate, usize)>> {
        let mut statement = self
            .connection
            .prepare("SELECT date, COUNT(*) FROM offices GROUP BY date ORDER BY date DESC")?;
        let rows = statement.query_map([], |row| {
            Ok((read_date(row, 0)?, row.get::<_, u32>(1)? as usize))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn annotation(
        &self,
        date: NaiveDate,
        hour: Hour,
        block_id: &str,
    ) -> Result<Option<Annotation>> {
        Ok(self
            .connection
            .query_row(
                "SELECT date, hour, block_id, quote, note, bookmarked, highlighted
             FROM annotations WHERE date = ?1 AND hour = ?2 AND block_id = ?3",
                params![date.to_string(), hour.key(), block_id],
                read_annotation,
            )
            .optional()?)
    }

    pub fn save_annotation(&self, annotation: &Annotation) -> Result<()> {
        validate_annotation(annotation)?;
        if annotation.is_empty() {
            self.connection.execute(
                "DELETE FROM annotations WHERE date = ?1 AND hour = ?2 AND block_id = ?3",
                params![
                    annotation.date.to_string(),
                    annotation.hour.key(),
                    annotation.block_id
                ],
            )?;
        } else {
            self.connection.execute(
                "INSERT INTO annotations
                    (date, hour, block_id, quote, note, bookmarked, highlighted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (date, hour, block_id) DO UPDATE SET
                    quote = CASE WHEN annotations.quote = '' THEN excluded.quote ELSE annotations.quote END,
                    note = excluded.note,
                    bookmarked = excluded.bookmarked,
                    highlighted = excluded.highlighted",
                params![
                    annotation.date.to_string(), annotation.hour.key(), annotation.block_id,
                    annotation.quote, annotation.note, annotation.bookmarked, annotation.highlighted,
                ],
            )?;
        }
        Ok(())
    }

    pub fn annotations(&self) -> Result<Vec<Annotation>> {
        let mut statement = self.connection.prepare(
            "SELECT date, hour, block_id, quote, note, bookmarked, highlighted
             FROM annotations ORDER BY date DESC, hour, block_id",
        )?;
        Ok(statement
            .query_map([], read_annotation)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn completed(&self, date: NaiveDate) -> Result<Vec<Hour>> {
        self.hours("SELECT hour FROM completed WHERE date = ?1", date)
    }

    pub fn set_completed(&self, date: NaiveDate, hour: Hour, completed: bool) -> Result<()> {
        validate_date(date)?;
        let sql = if completed {
            "INSERT INTO completed (date, hour) VALUES (?1, ?2) ON CONFLICT (date, hour) DO NOTHING"
        } else {
            "DELETE FROM completed WHERE date = ?1 AND hour = ?2"
        };
        self.connection
            .execute(sql, params![date.to_string(), hour.key()])?;
        Ok(())
    }

    pub fn position(&self, date: NaiveDate, hour: Hour) -> Result<f32> {
        Ok(self
            .connection
            .query_row(
                "SELECT offset FROM positions WHERE date = ?1 AND hour = ?2",
                params![date.to_string(), hour.key()],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0.0))
    }

    pub fn save_position(&self, date: NaiveDate, hour: Hour, offset: f32) -> Result<()> {
        validate_date(date)?;
        ensure!(
            offset.is_finite() && offset >= 0.0,
            "Reading position must be finite and nonnegative"
        );
        self.connection.execute(
            "INSERT INTO positions (date, hour, offset) VALUES (?1, ?2, ?3)
             ON CONFLICT (date, hour) DO UPDATE SET offset = excluded.offset",
            params![date.to_string(), hour.key(), offset],
        )?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn save_setting(&self, key: &str, value: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn export_to(&self, path: &Path) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let offices = {
            let mut statement =
                transaction.prepare("SELECT document FROM offices ORDER BY date, hour")?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .map(|row| Ok(serde_json::from_str(&row?)?))
                .collect::<Result<Vec<Office>>>()?
        };
        let completed = {
            let mut statement =
                transaction.prepare("SELECT date, hour FROM completed ORDER BY date, hour")?;
            statement
                .query_map([], |row| {
                    Ok(Completion {
                        date: read_date(row, 0)?,
                        hour: read_hour(row, 1)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?
        };
        let pack = Pack {
            version: VERSION,
            source_profile: SOURCE_PROFILE.into(),
            offices,
            annotations: self.annotations()?,
            completed,
        };
        transaction.commit()?;
        validate_pack(&pack)?;
        let bytes = serde_json::to_vec(&pack)?;
        ensure!(
            bytes.len() as u64 <= MAX_PACK_BYTES,
            "Backup exceeds the 64 MiB pack limit"
        );
        write_backup(path, &bytes)
    }

    pub fn import_from(&self, path: &Path) -> Result<usize> {
        let file = File::open(path).with_context(|| format!("Opening pack {}", path.display()))?;
        let metadata = file.metadata()?;
        ensure!(metadata.is_file(), "A library pack must be a regular file");
        ensure!(
            metadata.len() <= MAX_PACK_BYTES,
            "Pack exceeds the 64 MiB limit"
        );
        let mut bytes = Vec::new();
        file.take(MAX_PACK_BYTES + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_PACK_BYTES,
            "Pack exceeds the 64 MiB limit"
        );
        let pack: Pack = serde_json::from_slice(&bytes).context("Reading library pack JSON")?;
        validate_pack(&pack)?;
        let transaction = self.connection.unchecked_transaction()?;
        let mut inserted = 0;
        for office in pack.offices {
            inserted += transaction.execute(
                "INSERT INTO offices (date, hour, document) VALUES (?1, ?2, ?3)
                 ON CONFLICT (date, hour) DO NOTHING",
                params![
                    office.date.to_string(),
                    office.hour.key(),
                    serde_json::to_string(&office)?
                ],
            )?;
        }
        for annotation in pack.annotations {
            transaction.execute(
                "INSERT INTO annotations (date, hour, block_id, quote, note, bookmarked, highlighted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (date, hour, block_id) DO NOTHING",
                params![
                    annotation.date.to_string(), annotation.hour.key(), annotation.block_id,
                    annotation.quote, annotation.note, annotation.bookmarked, annotation.highlighted,
                ],
            )?;
        }
        for completion in pack.completed {
            transaction.execute(
                "INSERT INTO completed (date, hour) VALUES (?1, ?2) ON CONFLICT (date, hour) DO NOTHING",
                params![completion.date.to_string(), completion.hour.key()],
            )?;
        }
        transaction.commit()?;
        Ok(inserted)
    }

    fn hours(&self, sql: &str, date: NaiveDate) -> Result<Vec<Hour>> {
        let mut statement = self.connection.prepare(sql)?;
        let mut hours: Vec<Hour> = statement
            .query_map([date.to_string()], |row| read_hour(row, 0))?
            .collect::<rusqlite::Result<_>>()?;
        hours.sort_by_key(|hour| hour.index());
        Ok(hours)
    }
}

fn read_date(row: &Row<'_>, column: usize) -> rusqlite::Result<NaiveDate> {
    let value: String = row.get(column)?;
    value.parse().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn read_hour(row: &Row<'_>, column: usize) -> rusqlite::Result<Hour> {
    let value: String = row.get(column)?;
    Hour::ALL
        .into_iter()
        .find(|hour| hour.key() == value)
        .ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Unknown saved hour",
                )),
            )
        })
}

fn read_annotation(row: &Row<'_>) -> rusqlite::Result<Annotation> {
    Ok(Annotation {
        date: read_date(row, 0)?,
        hour: read_hour(row, 1)?,
        block_id: row.get(2)?,
        quote: row.get(3)?,
        note: row.get(4)?,
        bookmarked: row.get(5)?,
        highlighted: row.get(6)?,
    })
}

fn validate_text(text: &str, limit: usize, field: &str) -> Result<()> {
    ensure!(text.len() <= limit, "{field} exceeds its size limit");
    ensure!(
        !text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "{field} contains control characters"
    );
    Ok(())
}

fn validate_id(id: &str) -> Result<()> {
    validate_text(id, 512, "Paragraph or anchor ID")?;
    ensure!(
        !id.is_empty() && !id.chars().any(char::is_whitespace),
        "Paragraph and anchor IDs must be nonempty and contain no whitespace"
    );
    Ok(())
}

fn validate_source_url(value: &str) -> Result<()> {
    validate_text(value, 4096, "URL")?;
    ensure!(
        !value.chars().any(char::is_whitespace),
        "URL contains whitespace"
    );
    let url = reqwest::Url::parse(value).context("Invalid URL")?;
    ensure!(
        url.scheme() == "https" && url.host_str().is_some(),
        "Office source must use HTTPS"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "URL credentials are not allowed"
    );
    ensure!(
        matches!(url.host_str(), Some("ibreviary.com" | "www.ibreviary.com"))
            && url.port_or_known_default() == Some(443)
            && (url.path() == "/m2" || url.path().starts_with("/m2/")),
        "Office source must be https://ibreviary.com/m2 or its www equivalent"
    );
    for (key, value) in url.query_pairs() {
        if matches!(key.as_ref(), "lang" | "language") {
            ensure!(value == "en", "Office source must use the English profile");
        }
    }
    Ok(())
}

fn validate_office(office: &Office) -> Result<()> {
    validate_date(office.date)?;
    validate_text(&office.celebration, 16 * 1024, "Celebration")?;
    ensure!(
        !office.celebration.trim().is_empty(),
        "Office has no celebration"
    );
    validate_source_url(&office.source_url)?;
    ensure!(
        !office.sections.is_empty() && office.sections.len() <= 512,
        "Office must have between 1 and 512 sections"
    );
    let mut ids = HashSet::new();
    let anchors: HashSet<_> = office
        .sections
        .iter()
        .flat_map(|section| &section.blocks)
        .flat_map(|block| &block.anchors)
        .map(String::as_str)
        .collect();
    let mut text_bytes = 0;
    for section in &office.sections {
        validate_text(&section.title, 16 * 1024, "Section title")?;
        ensure!(
            !section.blocks.is_empty(),
            "Office section has no paragraphs"
        );
        text_bytes += section.title.len();
        for block in &section.blocks {
            validate_id(&block.id)?;
            ensure!(ids.insert(&block.id), "Duplicate paragraph ID");
            ensure!(ids.len() <= 20_000, "Office has too many paragraphs");
            ensure!(block.anchors.len() <= 256, "Paragraph has too many anchors");
            for anchor in &block.anchors {
                validate_id(anchor)?;
            }
            ensure!(
                !block.runs.is_empty() && block.runs.len() <= 4096,
                "Paragraph must have between 1 and 4096 text runs"
            );
            let mut has_text = false;
            for run in &block.runs {
                validate_text(&run.text, MAX_TEXT_BYTES, "Paragraph text")?;
                has_text |= !run.text.trim().is_empty();
                text_bytes += run.text.len();
                ensure!(
                    text_bytes <= 8 * MAX_TEXT_BYTES,
                    "Office text exceeds the 8 MiB limit"
                );
                if let RunKind::Link(url) = &run.kind {
                    let anchor = url
                        .strip_prefix('#')
                        .context("Office links must be local anchors")?;
                    validate_id(anchor)?;
                    ensure!(
                        anchors.contains(anchor),
                        "Office link points to a missing anchor"
                    );
                }
            }
            ensure!(has_text, "Paragraph has no text");
        }
    }
    Ok(())
}

fn validate_annotation(annotation: &Annotation) -> Result<()> {
    validate_date(annotation.date)?;
    validate_id(&annotation.block_id)?;
    validate_text(&annotation.quote, MAX_TEXT_BYTES, "Annotation quote")?;
    validate_text(&annotation.note, MAX_TEXT_BYTES, "Annotation note")
}

fn validate_pack(pack: &Pack) -> Result<()> {
    ensure!(
        pack.version == VERSION,
        "Unsupported pack schema version {}",
        pack.version
    );
    ensure!(
        pack.source_profile == SOURCE_PROFILE,
        "Pack must use the modern Roman English iBreviary profile"
    );
    let mut offices = HashSet::new();
    for office in &pack.offices {
        validate_office(office)?;
        ensure!(
            offices.insert((office.date, office.hour)),
            "Duplicate office in pack"
        );
    }
    let mut annotations = HashSet::new();
    for annotation in &pack.annotations {
        validate_annotation(annotation)?;
        ensure!(!annotation.is_empty(), "Pack contains an empty annotation");
        ensure!(
            annotations.insert((annotation.date, annotation.hour, &annotation.block_id)),
            "Duplicate annotation in pack"
        );
    }
    let mut completed = HashSet::new();
    for completion in &pack.completed {
        validate_date(completion.date)?;
        ensure!(
            completed.insert((completion.date, completion.hour)),
            "Duplicate completion in pack"
        );
    }
    Ok(())
}

fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

struct PendingBackup(PathBuf);

impl Drop for PendingBackup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn write_backup(path: &Path, bytes: &[u8]) -> Result<()> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let parent = parent_directory(path);
    fs::create_dir_all(parent)?;
    let name = path.file_name().context("Backup path must name a file")?;
    for _ in 0..100 {
        let mut temporary_name = name.to_os_string();
        temporary_name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary_path = parent.join(temporary_name);
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        let pending = PendingBackup(temporary_path);
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // Publishing a hard link is atomic and cannot replace an existing backup.
        fs::hard_link(&pending.0, path)
            .with_context(|| format!("Publishing backup {} without overwriting", path.display()))?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        return Ok(());
    }
    bail!("Could not create a temporary backup file")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Run, Section};
    use serde_json::{Value, json};
    use std::sync::{Arc, Barrier};
    use tempfile::tempdir;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
    }

    fn synthetic_office(hour: Hour) -> Office {
        Office {
            date: date(),
            hour,
            celebration: "Synthetic calendar fixture".into(),
            source_url: format!(
                "https://www.ibreviary.com/m2/breviario.php?s={}",
                hour.source_id()
            ),
            sections: vec![Section {
                title: "Synthetic section".into(),
                blocks: vec![Block {
                    id: "synthetic-paragraph".into(),
                    anchors: vec!["synthetic-anchor".into()],
                    runs: vec![
                        Run {
                            text: "Synthetic rubric. ".into(),
                            kind: RunKind::Rubric,
                        },
                        Run {
                            text: "Synthetic body. ".into(),
                            kind: RunKind::Text,
                        },
                        Run {
                            text: "Synthetic emphasis. ".into(),
                            kind: RunKind::Italic,
                        },
                        Run {
                            text: "Synthetic credit. ".into(),
                            kind: RunKind::Small,
                        },
                        Run {
                            text: "Synthetic link.".into(),
                            kind: RunKind::Link("#synthetic-anchor".into()),
                        },
                    ],
                }],
            }],
        }
    }

    fn synthetic_annotation(hour: Hour) -> Annotation {
        Annotation {
            date: date(),
            hour,
            block_id: "synthetic-paragraph".into(),
            quote: "Synthetic original quote".into(),
            note: "Synthetic personal note".into(),
            bookmarked: true,
            highlighted: true,
        }
    }

    fn synthetic_pack() -> Pack {
        Pack {
            version: VERSION,
            source_profile: SOURCE_PROFILE.into(),
            offices: vec![
                synthetic_office(Hour::Morning),
                synthetic_office(Hour::Evening),
            ],
            annotations: vec![
                synthetic_annotation(Hour::Morning),
                synthetic_annotation(Hour::Evening),
            ],
            completed: vec![Completion {
                date: date(),
                hour: Hour::Morning,
            }],
        }
    }

    fn write_pack(path: &Path, pack: &Pack) {
        fs::write(path, serde_json::to_vec(pack).unwrap()).unwrap();
    }

    #[test]
    fn content_annotations_progress_and_settings_survive_reopening() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("nested/library.sqlite3");
        let missing = date().succ_opt().unwrap();
        let morning = synthetic_office(Hour::Morning);
        {
            let library = Library::open(&path)?;
            assert!(library.save_office(&synthetic_office(Hour::Night))?);
            assert!(library.save_office(&morning)?);
            assert!(!library.save_office(&morning)?);
            library.save_annotation(&synthetic_annotation(Hour::Morning))?;
            library.set_completed(date(), Hour::Night, true)?;
            library.set_completed(date(), Hour::Morning, true)?;
            library.set_completed(date(), Hour::Morning, true)?;
            library.save_position(date(), Hour::Morning, 1234.5)?;
            library.save_setting("theme", "light")?;
            library.save_setting("theme", "dark")?;
        }
        let library = Library::open(&path)?;
        let saved = library.office(date(), Hour::Morning)?.unwrap();
        assert_eq!(serde_json::to_value(saved)?, serde_json::to_value(morning)?);
        assert_eq!(
            library.availability(date())?,
            vec![Hour::Morning, Hour::Night]
        );
        assert_eq!(library.downloaded_dates()?, vec![(date(), 2)]);
        let annotation = library
            .annotation(date(), Hour::Morning, "synthetic-paragraph")?
            .unwrap();
        assert_eq!(annotation.quote, "Synthetic original quote");
        assert_eq!(annotation.note, "Synthetic personal note");
        assert!(annotation.bookmarked && annotation.highlighted);
        assert_eq!(library.annotations()?.len(), 1);
        assert_eq!(library.completed(date())?, vec![Hour::Morning, Hour::Night]);
        assert_eq!(library.position(date(), Hour::Morning)?, 1234.5);
        assert_eq!(library.setting("theme")?.as_deref(), Some("dark"));
        assert!(library.setting("unknown")?.is_none());
        assert!(library.office(missing, Hour::Morning)?.is_none());
        assert!(library.availability(missing)?.is_empty());
        assert!(library.completed(missing)?.is_empty());
        assert_eq!(library.position(missing, Hour::Morning)?, 0.0);
        assert!(
            library
                .annotation(missing, Hour::Morning, "synthetic-paragraph")?
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn local_updates_preserve_original_quote_and_delete_only_the_selected_annotation() -> Result<()>
    {
        let directory = tempdir()?;
        let path = directory.path().join("library.sqlite3");
        {
            let library = Library::open(&path)?;
            let mut annotation = synthetic_annotation(Hour::Morning);
            library.save_annotation(&annotation)?;
            library.save_annotation(&synthetic_annotation(Hour::Evening))?;
            for quote in ["", "Synthetic replacement quote"] {
                annotation.quote = quote.into();
                annotation.note = "Synthetic revised note".into();
                annotation.highlighted = false;
                library.save_annotation(&annotation)?;
                let saved = library
                    .annotation(date(), Hour::Morning, &annotation.block_id)?
                    .unwrap();
                assert_eq!(saved.quote, "Synthetic original quote");
                assert_eq!(saved.note, "Synthetic revised note");
                assert!(saved.bookmarked && !saved.highlighted);
            }
            annotation.note.clear();
            annotation.bookmarked = false;
            library.save_annotation(&annotation)?;
        }
        let library = Library::open(&path)?;
        assert!(
            library
                .annotation(date(), Hour::Morning, "synthetic-paragraph")?
                .is_none()
        );
        assert_eq!(library.annotations()?.len(), 1);
        assert!(
            library
                .annotation(date(), Hour::Evening, "synthetic-paragraph")?
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn saved_offices_cannot_replace_text_or_remove_annotations() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("library.sqlite3");
        let original = synthetic_office(Hour::Morning);
        {
            let library = Library::open(&path)?;
            assert!(library.save_office(&original)?);
            library.save_annotation(&synthetic_annotation(Hour::Morning))?;
            let mut changed = original.clone();
            changed.sections[0].blocks[0].runs[0].text = "Synthetic replacement text".into();
            assert!(!library.save_office(&changed)?);
        }
        let library = Library::open(&path)?;
        assert_eq!(
            library.office(date(), Hour::Morning)?.unwrap().text(),
            original.text()
        );
        assert_eq!(library.annotations()?[0].note, "Synthetic personal note");
        Ok(())
    }

    #[test]
    fn concurrent_saves_insert_one_complete_office() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("library.sqlite3");
        let first = Library::open(&path)?;
        let second = Library::open(&path)?;
        let barrier = Arc::new(Barrier::new(2));
        let workers: Vec<_> = [first, second]
            .into_iter()
            .enumerate()
            .map(|(index, library)| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut office = synthetic_office(Hour::Morning);
                    office.celebration = format!("Synthetic contender {index}");
                    barrier.wait();
                    library.save_office(&office)
                })
            })
            .collect();
        let mut inserted = 0;
        for worker in workers {
            inserted += usize::from(worker.join().unwrap()?);
        }
        assert_eq!(inserted, 1);
        let library = Library::open(&path)?;
        assert_eq!(library.downloaded_dates()?, vec![(date(), 1)]);
        let office = library.office(date(), Hour::Morning)?.unwrap();
        assert!(matches!(
            office.celebration.as_str(),
            "Synthetic contender 0" | "Synthetic contender 1"
        ));
        assert_eq!(office.sections[0].blocks[0].runs.len(), 5);
        Ok(())
    }

    #[test]
    fn completion_can_be_cleared_and_invalid_positions_do_not_replace_progress() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("library.sqlite3");
        {
            let library = Library::open(&path)?;
            library.set_completed(date(), Hour::Morning, true)?;
            library.set_completed(date(), Hour::Morning, false)?;
            library.set_completed(date(), Hour::Morning, false)?;
            library.save_position(date(), Hour::Morning, 8.5)?;
            for offset in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                assert!(
                    library
                        .save_position(date(), Hour::Morning, offset)
                        .is_err()
                );
            }
        }
        let library = Library::open(&path)?;
        assert!(library.completed(date())?.is_empty());
        assert_eq!(library.position(date(), Hour::Morning)?, 8.5);
        Ok(())
    }

    #[test]
    fn export_import_round_trip_and_retries_preserve_personal_data() -> Result<()> {
        let directory = tempdir()?;
        let source_path = directory.path().join("source.sqlite3");
        let target_path = directory.path().join("target.sqlite3");
        let backup_path = directory.path().join("backups/library.json");
        {
            let source = Library::open(&source_path)?;
            for office in &synthetic_pack().offices {
                source.save_office(office)?;
            }
            for annotation in &synthetic_pack().annotations {
                source.save_annotation(annotation)?;
            }
            source.set_completed(date(), Hour::Morning, true)?;
        }
        Library::open(&source_path)?.export_to(&backup_path)?;
        {
            let target = Library::open(&target_path)?;
            assert_eq!(target.import_from(&backup_path)?, 2);
            assert_eq!(target.import_from(&backup_path)?, 0);
        }
        let target = Library::open(&target_path)?;
        assert_eq!(target.downloaded_dates()?, vec![(date(), 2)]);
        assert_eq!(
            target.office(date(), Hour::Morning)?.unwrap().text(),
            synthetic_office(Hour::Morning).text()
        );
        assert_eq!(target.annotations()?.len(), 2);
        let annotation = target
            .annotation(date(), Hour::Morning, "synthetic-paragraph")?
            .unwrap();
        assert_eq!(annotation.quote, "Synthetic original quote");
        assert_eq!(annotation.note, "Synthetic personal note");
        assert!(annotation.bookmarked && annotation.highlighted);
        assert_eq!(target.completed(date())?, vec![Hour::Morning]);
        assert!(
            target
                .office(date().succ_opt().unwrap(), Hour::Morning)?
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn import_never_overwrites_existing_documents_annotations_settings_or_position() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("library.sqlite3");
        let pack_path = directory.path().join("pack.json");
        write_pack(&pack_path, &synthetic_pack());
        {
            let target = Library::open(&path)?;
            let mut office = synthetic_office(Hour::Morning);
            office.celebration = "Synthetic local celebration".into();
            target.save_office(&office)?;
            let mut annotation = synthetic_annotation(Hour::Morning);
            annotation.quote = "Synthetic local quote".into();
            annotation.note = "Synthetic local note".into();
            annotation.bookmarked = false;
            annotation.highlighted = false;
            target.save_annotation(&annotation)?;
            target.set_completed(date(), Hour::Night, true)?;
            target.save_position(date(), Hour::Morning, 42.0)?;
            target.save_setting("theme", "local")?;
            assert_eq!(target.import_from(&pack_path)?, 1);
        }
        let target = Library::open(&path)?;
        assert_eq!(
            target.office(date(), Hour::Morning)?.unwrap().celebration,
            "Synthetic local celebration"
        );
        let annotation = target
            .annotation(date(), Hour::Morning, "synthetic-paragraph")?
            .unwrap();
        assert_eq!(annotation.quote, "Synthetic local quote");
        assert_eq!(annotation.note, "Synthetic local note");
        assert!(!annotation.bookmarked && !annotation.highlighted);
        assert_eq!(target.annotations()?.len(), 2);
        assert_eq!(target.completed(date())?, vec![Hour::Morning, Hour::Night]);
        assert_eq!(target.position(date(), Hour::Morning)?, 42.0);
        assert_eq!(target.setting("theme")?.as_deref(), Some("local"));
        Ok(())
    }

    #[test]
    fn export_refuses_to_replace_a_backup_and_leaves_no_temporary_files() -> Result<()> {
        let directory = tempdir()?;
        let backup_directory = directory.path().join("backups");
        let backup_path = backup_directory.join("library.json");
        let library = Library::open(&directory.path().join("library.sqlite3"))?;
        library.save_office(&synthetic_office(Hour::Morning))?;
        library.export_to(&backup_path)?;
        let original = fs::read(&backup_path)?;
        library.save_office(&synthetic_office(Hour::Night))?;
        assert!(library.export_to(&backup_path).is_err());
        assert_eq!(fs::read(&backup_path)?, original);
        assert_eq!(fs::read_dir(&backup_directory)?.count(), 1);
        Ok(())
    }

    #[test]
    fn invalid_packs_cannot_partially_import() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("library.sqlite3");
        let pack_path = directory.path().join("invalid.json");
        let baseline = serde_json::to_value(synthetic_pack())?;
        let cases = [
            ("/version", json!(2)),
            ("/source_profile", json!("ibreviary-modern-roman-italian")),
            ("/offices/1/date", json!("2026-02-30")),
            ("/offices/1/date", json!("0000-01-01")),
            ("/offices/1/hour", json!("unknown")),
            ("/offices/1/celebration", json!("  ")),
            ("/offices/1/sections", json!([])),
            ("/offices/1/sections/0/blocks", json!([])),
            ("/offices/1/sections/0/blocks/0/id", json!("")),
            (
                "/offices/1/sections/0/blocks/0/anchors/0",
                json!("bad anchor"),
            ),
            ("/offices/1/sections/0/blocks/0/runs", json!([])),
            ("/offices/1/sections/0/blocks/0/runs/0/text", json!("\u{0}")),
            (
                "/offices/1/sections/0/blocks/0/runs/0/kind",
                json!({"link":"javascript:alert(1)"}),
            ),
            (
                "/offices/1/sections/0/blocks/0/runs/0/kind",
                json!({"link":"file:///tmp/synthetic"}),
            ),
            (
                "/offices/1/sections/0/blocks/0/runs/0/kind",
                json!({"link":"shell:synthetic"}),
            ),
            (
                "/offices/1/sections/0/blocks/0/runs/0/kind",
                json!({"link":"https://ibreviary.com/m2"}),
            ),
            (
                "/offices/1/sections/0/blocks/0/runs/0/kind",
                json!({"link":"#missing-anchor"}),
            ),
            ("/annotations/1/date", json!("not-a-date")),
            ("/annotations/1/block_id", json!("")),
            ("/annotations/1/note", json!("\u{0}")),
            ("/annotations/1/highlighted", json!("yes")),
            ("/completed/0/date", json!("2026-13-01")),
            ("/completed/0/hour", json!("invalid")),
        ];
        for (pointer, invalid) in cases {
            let mut pack = baseline.clone();
            *pack.pointer_mut(pointer).unwrap() = invalid;
            fs::write(&pack_path, serde_json::to_vec(&pack)?)?;
            {
                let library = Library::open(&path)?;
                assert!(
                    library.import_from(&pack_path).is_err(),
                    "Accepted {pointer}"
                );
            }
            let reopened = Library::open(&path)?;
            assert!(
                reopened.downloaded_dates()?.is_empty(),
                "Partially imported {pointer}"
            );
            assert!(reopened.annotations()?.is_empty());
            assert!(reopened.completed(date())?.is_empty());
        }
        Ok(())
    }

    #[test]
    fn source_profile_and_url_boundaries_are_enforced() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("library.sqlite3");
        let pack_path = directory.path().join("pack.json");
        let library = Library::open(&path)?;
        for source in [
            "http://ibreviary.com/m2",
            "https://ibreviary.com.evil.invalid/m2",
            "https://evil.invalid/m2",
            "https://ibreviary.com/m2-other",
            "https://ibreviary.com/m2/../elsewhere",
            "https://user:password@ibreviary.com/m2",
            "https://ibreviary.com:444/m2",
            "https://ibreviary.com/m2?lang=it",
            "file:///tmp/synthetic",
        ] {
            let mut pack = synthetic_pack();
            pack.offices[1].source_url = source.into();
            write_pack(&pack_path, &pack);
            assert!(
                library.import_from(&pack_path).is_err(),
                "Accepted {source}"
            );
            assert!(library.downloaded_dates()?.is_empty());
        }
        for source in [
            "https://ibreviary.com/m2",
            "https://www.ibreviary.com/m2/",
            "https://ibreviary.com/m2/breviario.php?lang=en",
        ] {
            let mut office = synthetic_office(Hour::Morning);
            office.source_url = source.into();
            validate_office(&office)?;
        }
        Ok(())
    }

    #[test]
    fn duplicate_pack_identities_and_oversized_fields_are_rejected() -> Result<()> {
        let directory = tempdir()?;
        let pack_path = directory.path().join("pack.json");
        let library = Library::open(&directory.path().join("library.sqlite3"))?;
        let baseline = serde_json::to_value(synthetic_pack())?;
        for pointer in [
            "/offices",
            "/annotations",
            "/completed",
            "/offices/0/sections/0/blocks",
        ] {
            let mut pack = baseline.clone();
            let array = pack.pointer_mut(pointer).unwrap().as_array_mut().unwrap();
            array.push(array[0].clone());
            fs::write(&pack_path, serde_json::to_vec(&pack)?)?;
            assert!(
                library.import_from(&pack_path).is_err(),
                "Accepted duplicate {pointer}"
            );
            assert!(library.downloaded_dates()?.is_empty());
        }
        let mut pack = synthetic_pack();
        pack.annotations[1].note = "x".repeat(MAX_TEXT_BYTES + 1);
        write_pack(&pack_path, &pack);
        assert!(library.import_from(&pack_path).is_err());
        assert!(library.downloaded_dates()?.is_empty());
        Ok(())
    }

    #[test]
    fn database_failure_rolls_back_documents_annotations_and_completion_together() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("library.sqlite3");
        let pack_path = directory.path().join("pack.json");
        write_pack(&pack_path, &synthetic_pack());
        {
            let library = Library::open(&path)?;
            library.connection.execute_batch(
                "CREATE TRIGGER fail_completion BEFORE INSERT ON completed
                 BEGIN SELECT RAISE(ABORT, 'Synthetic write failure'); END;",
            )?;
            assert!(library.import_from(&pack_path).is_err());
        }
        let library = Library::open(&path)?;
        assert!(library.downloaded_dates()?.is_empty());
        assert!(library.annotations()?.is_empty());
        assert!(library.completed(date())?.is_empty());
        library
            .connection
            .execute_batch("DROP TRIGGER fail_completion")?;
        assert_eq!(library.import_from(&pack_path)?, 2);
        Ok(())
    }

    #[test]
    fn oversized_malformed_and_trailing_input_is_rejected() -> Result<()> {
        let directory = tempdir()?;
        let pack_path = directory.path().join("pack.json");
        let library = Library::open(&directory.path().join("library.sqlite3"))?;
        File::create(&pack_path)?.set_len(MAX_PACK_BYTES + 1)?;
        assert!(library.import_from(&pack_path).is_err());
        for input in [b"{".as_slice(), b"null", b"[]"] {
            fs::write(&pack_path, input)?;
            assert!(library.import_from(&pack_path).is_err());
        }
        let mut json = serde_json::to_vec(&synthetic_pack())?;
        json.extend_from_slice(b" {}");
        fs::write(&pack_path, json)?;
        assert!(library.import_from(&pack_path).is_err());
        let mut unknown: Value = serde_json::to_value(synthetic_pack())?;
        unknown["unknown"] = json!(true);
        fs::write(&pack_path, serde_json::to_vec(&unknown)?)?;
        assert!(library.import_from(&pack_path).is_err());
        assert!(library.downloaded_dates()?.is_empty());
        Ok(())
    }

    #[test]
    fn unsupported_database_version_is_not_modified() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("library.sqlite3");
        {
            let connection = Connection::open(&path)?;
            connection.pragma_update(None, "user_version", 99)?;
        }
        assert!(Library::open(&path).is_err());
        let connection = Connection::open(path)?;
        assert_eq!(
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            99
        );
        Ok(())
    }
}
