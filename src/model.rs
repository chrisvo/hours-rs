use chrono::{Datelike, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};

pub fn validate_date(date: NaiveDate) -> anyhow::Result<()> {
    anyhow::ensure!(
        (1..=9999).contains(&date.year()),
        "Date must be within years 0001 through 9999"
    );
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Hour {
    Readings,
    #[default]
    Morning,
    Daytime,
    Evening,
    Night,
}

impl Hour {
    pub const ALL: [Self; 5] = [
        Self::Readings,
        Self::Morning,
        Self::Daytime,
        Self::Evening,
        Self::Night,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Readings => "Office of Readings",
            Self::Morning => "Morning Prayer",
            Self::Daytime => "Daytime Prayer",
            Self::Evening => "Evening Prayer",
            Self::Night => "Night Prayer",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Readings => "At any time",
            Self::Morning => "Lauds · At the start of the day",
            Self::Daytime => "Terce, Sext & None",
            Self::Evening => "Vespers · As evening falls",
            Self::Night => "Compline · Before sleep",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Readings => "readings",
            Self::Morning => "morning",
            Self::Daytime => "daytime",
            Self::Evening => "evening",
            Self::Night => "night",
        }
    }

    pub fn source_id(self) -> &'static str {
        match self {
            Self::Readings => "ufficio_delle_letture",
            Self::Morning => "lodi",
            Self::Daytime => "ora_media",
            Self::Evening => "vespri",
            Self::Night => "compieta",
        }
    }

    pub fn suggested() -> Self {
        Self::at_hour(chrono::Local::now().hour())
    }

    pub fn at_hour(hour: u32) -> Self {
        match hour {
            5..=10 => Self::Morning,
            11..=15 => Self::Daytime,
            16..=20 => Self::Evening,
            _ => Self::Night,
        }
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|h| *h == self).unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    Text,
    Rubric,
    Italic,
    Small,
    Link(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub text: String,
    pub kind: RunKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub anchors: Vec<String>,
    pub runs: Vec<Run>,
}

impl Block {
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    pub fn id_for(text: &str, occurrence: usize) -> String {
        let hash = text.bytes().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
        });
        format!("{hash:016x}-{occurrence}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub title: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Office {
    pub date: NaiveDate,
    pub hour: Hour,
    pub celebration: String,
    pub source_url: String,
    pub sections: Vec<Section>,
}

impl Office {
    pub fn block(&self, id: &str) -> Option<&Block> {
        self.sections
            .iter()
            .flat_map(|s| &s.blocks)
            .find(|b| b.id == id)
    }

    pub fn search(&self, query: &str) -> Vec<(usize, &Block)> {
        let words: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        self.sections
            .iter()
            .enumerate()
            .flat_map(|(i, s)| s.blocks.iter().map(move |b| (i, b)))
            .filter(|(_, b)| {
                let text = b.text().to_lowercase();
                words.iter().all(|w| text.contains(w))
            })
            .collect()
    }

    pub fn text(&self) -> String {
        let mut result = format!(
            "{}\n{}\n{}\n\n",
            self.hour.title(),
            self.date,
            self.celebration
        );
        for section in &self.sections {
            result.push_str(&section.title);
            result.push_str("\n\n");
            for block in &section.blocks {
                result.push_str(&block.text());
                result.push_str("\n\n");
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_dates_exclude_extended_years_at_every_input_boundary() {
        for text in ["0001-01-01", "9999-12-31", "2026-10-23"] {
            assert!(validate_date(text.parse().unwrap()).is_ok());
        }
        for text in ["0000-01-01", "+262142-12-01", "-262143-01-01"] {
            assert!(validate_date(text.parse().unwrap()).is_err());
        }
    }

    #[test]
    fn suggestions_follow_the_day_without_limiting_access() {
        assert_eq!(Hour::at_hour(7), Hour::Morning);
        assert_eq!(Hour::at_hour(12), Hour::Daytime);
        assert_eq!(Hour::at_hour(18), Hour::Evening);
        assert_eq!(Hour::at_hour(23), Hour::Night);
        assert_eq!(Hour::Night.next(), Hour::Readings);
        assert_eq!(Hour::Readings.previous(), Hour::Night);
    }
}
