use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use chrono::{Datelike, NaiveDate};
use ego_tree::NodeRef;
use reqwest::blocking::Client;
use scraper::{ElementRef, Html, Node, Selector};

use crate::model::{Block, Hour, Office, Run, RunKind, Section};

const BASE_URL: &str = "https://ibreviary.com/m2";

pub fn download_date(
    date: NaiveDate,
    hours: &[Hour],
    cancelled: &AtomicBool,
    mut emit: impl FnMut(Office),
) -> Result<()> {
    check_cancelled(cancelled)?;
    if hours.is_empty() {
        return Ok(());
    }

    let client = Client::builder()
        .cookie_store(true)
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(25))
        .user_agent("hours-rs/0.1 (personal offline prayer library)")
        .build()
        .context("Could not start the iBreviary download session")?;
    let form = [
        ("lang", "en".to_owned()),
        ("giorno", date.day().to_string()),
        ("mese", date.month().to_string()),
        ("anno", date.year().to_string()),
        ("ok", "ok".to_owned()),
    ];
    check_cancelled(cancelled)?;
    client
        .post(format!("{BASE_URL}/opzioni.php"))
        .form(&form)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .and_then(reqwest::blocking::Response::text)
        .with_context(|| format!("Could not select English and {date} on iBreviary"))?;

    let fetch = |url: &str, purpose: &str| -> Result<String> {
        check_cancelled(cancelled)?;
        let html = client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .and_then(reqwest::blocking::Response::text)
            .with_context(|| format!("Could not download {purpose} for {date} from iBreviary"))?;
        check_cancelled(cancelled)?;
        Ok(html)
    };
    let calendar_url = format!("{BASE_URL}/breviario.php");
    for &hour in hours {
        let before = fetch(&calendar_url, "the calendar before the office")?;
        let celebration = parse_calendar(&before, date)
            .with_context(|| format!("Could not verify the date before {}", hour.title()))?;
        let source_url = format!("{calendar_url}?s={}", hour.source_id());
        let html = fetch(&source_url, hour.title())?;
        let after = fetch(&calendar_url, "the calendar after the office")?;
        let confirmed = parse_calendar(&after, date)
            .with_context(|| format!("Could not verify the date after {}", hour.title()))?;
        ensure!(
            celebration == confirmed,
            "The iBreviary celebration changed while downloading {} for {date}. Please retry the download.",
            hour.title()
        );
        let office = parse_office(&html, date, hour, celebration, source_url)
            .with_context(|| format!("Could not read {} for {date}", hour.title()))?;
        check_cancelled(cancelled)?;
        emit(office);
    }
    Ok(())
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    ensure!(!cancelled.load(Ordering::Relaxed), "Download cancelled.");
    Ok(())
}

pub fn parse_calendar(html: &str, expected: NaiveDate) -> Result<String> {
    let document = complete_document(html)?;
    let content = content_root(&document)?;
    validate_english_title(content)?;
    let mut paragraphs = content
        .child_elements()
        .filter(|el| el.value().name() == "p");
    let date_text = paragraphs
        .next()
        .map(element_text)
        .context("The iBreviary calendar has no date. No office was accepted.")?;
    let actual = NaiveDate::parse_from_str(&date_text, "%A, %e %B %Y").context(
        "The iBreviary calendar does not contain a valid English date. No office was accepted.",
    )?;
    ensure!(
        actual == expected,
        "iBreviary returned {actual}, but {expected} was requested. No office was accepted."
    );
    let celebration = paragraphs
        .next()
        .context("The iBreviary calendar has no celebration.")?;
    ensure!(
        celebration
            .select(&selector("a, .sezione"))
            .next()
            .is_none(),
        "The iBreviary calendar has no celebration."
    );
    let celebration = element_text(celebration);
    ensure!(
        !celebration.is_empty() && !celebration.starts_with("Tipo:"),
        "The iBreviary calendar has no celebration."
    );
    Ok(celebration)
}

pub fn parse_office(
    html: &str,
    date: NaiveDate,
    hour: Hour,
    celebration: String,
    source_url: String,
) -> Result<Office> {
    let document = complete_document(html)?;
    let content = content_root(&document)?;
    validate_english_title(content)?;
    let hour_selector = selector(".sezione");
    let labels: Vec<_> = content.select(&hour_selector).map(element_text).collect();
    let expected = match hour {
        Hour::Daytime => "Daytime Prayers",
        _ => hour.title(),
    };
    ensure!(
        labels.len() == 1 && labels[0] == expected,
        "iBreviary did not return the expected English {expected}. No office was accepted."
    );

    let mut parser = OfficeParser::default();
    for child in content.children() {
        parser.visit(child, &RunKind::Text, 0)?;
    }
    parser.finish_block();
    if let Some(last) = parser.sections.last_mut().and_then(|s| s.blocks.last_mut()) {
        last.anchors.append(&mut parser.anchors);
    }
    ensure!(
        parser.heading_count >= 2 && parser.sections.iter().all(|s| !s.blocks.is_empty()),
        "The iBreviary office has missing or empty sections. Please retry the download."
    );
    let blocks: Vec<_> = parser.sections.iter().flat_map(|s| &s.blocks).collect();
    let body_letters = blocks
        .iter()
        .flat_map(|b| &b.runs)
        .filter(|r| matches!(r.kind, RunKind::Text | RunKind::Italic))
        .flat_map(|r| r.text.chars())
        .filter(|c| c.is_alphabetic())
        .count();
    ensure!(
        body_letters >= 80,
        "The iBreviary office contains too little prayer text and may be incomplete. Please retry the download."
    );
    let anchors: HashSet<_> = blocks
        .iter()
        .flat_map(|b| b.anchors.iter().map(String::as_str))
        .collect();
    for run in blocks.iter().flat_map(|b| &b.runs) {
        if let RunKind::Link(target) = &run.kind {
            ensure!(
                target
                    .strip_prefix('#')
                    .is_some_and(|name| anchors.contains(name)),
                "The iBreviary office is missing a linked alternative or section. Please retry the download."
            );
        }
    }
    Ok(Office {
        date,
        hour,
        celebration,
        source_url,
        sections: parser.sections,
    })
}

fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("provider selectors are static CSS")
}

fn complete_document(html: &str) -> Result<Html> {
    let lower = html.trim_end().to_ascii_lowercase();
    ensure!(
        lower.ends_with("</html>") && lower.contains("</body>"),
        "The iBreviary page ended before the document was complete. Please retry the download."
    );
    let document = Html::parse_document(html);
    let mut menu_error_allowed = document
        .select(&selector("#menu_bar ul ul"))
        .next()
        .is_some();
    for error in &document.errors {
        if menu_error_allowed && error.as_ref() == "Unexpected open element" {
            menu_error_allowed = false;
        } else {
            bail!("The iBreviary page contains malformed HTML. Please retry the download.");
        }
    }
    Ok(document)
}

fn content_root(document: &Html) -> Result<ElementRef<'_>> {
    let content_selector = selector("#contenuto > .inner");
    let mut roots = document.select(&content_selector);
    let content = roots
        .next()
        .context("The iBreviary page has no office content.")?;
    ensure!(
        roots.next().is_none(),
        "The iBreviary page has ambiguous office content."
    );
    Ok(content)
}

fn validate_english_title(content: ElementRef<'_>) -> Result<()> {
    let title_selector = selector("h1");
    let mut titles = content.select(&title_selector);
    ensure!(
        titles
            .next()
            .is_some_and(|el| element_text(el) == "Breviary")
            && titles.next().is_none(),
        "iBreviary did not return an English Breviary page. Please retry the download."
    );
    Ok(())
}

fn element_text(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Default)]
struct OfficeParser {
    sections: Vec<Section>,
    runs: Vec<Run>,
    anchors: Vec<String>,
    occurrences: HashMap<String, usize>,
    heading_count: usize,
    consecutive_breaks: u8,
}

impl OfficeParser {
    fn visit(&mut self, node: NodeRef<'_, Node>, inherited: &RunKind, depth: usize) -> Result<()> {
        ensure!(
            depth < 256,
            "The iBreviary page has malformed, deeply nested content."
        );
        let Some(element) = ElementRef::wrap(node) else {
            if let Node::Text(text) = node.value() {
                self.text(text, inherited);
            }
            return Ok(());
        };
        let value = element.value();
        let tag = value.name();
        if is_site_navigation(element)
            || matches!(
                tag,
                "script" | "style" | "noscript" | "nav" | "form" | "button" | "iframe"
            )
            || has_class(element, "sezione")
            || (tag == "h1" && element_text(element) == "Breviary")
        {
            return Ok(());
        }
        let is_block = matches!(tag, "p" | "div" | "li" | "blockquote" | "hr" | "h2" | "h3");
        if is_block {
            self.finish_block();
        }
        for anchor in value
            .attr("id")
            .into_iter()
            .chain(value.attr("name").filter(|_| tag == "a"))
        {
            self.anchor(anchor);
        }
        if has_class(element, "capolettera_piccolo") {
            self.finish_block();
            let title = element_text(element);
            ensure!(
                !title.is_empty(),
                "The iBreviary office has an empty section heading."
            );
            for descendant in element.descendent_elements().skip(1) {
                for anchor in descendant.attr("id").into_iter().chain(
                    descendant
                        .attr("name")
                        .filter(|_| descendant.value().name() == "a"),
                ) {
                    self.anchor(anchor);
                }
            }
            self.sections.push(Section {
                title,
                blocks: Vec::new(),
            });
            self.heading_count += 1;
            return Ok(());
        }
        if tag == "br" {
            self.line_break(inherited);
            return Ok(());
        }
        let kind = if matches!(inherited, RunKind::Link(_)) {
            inherited.clone()
        } else if let Some(target) = value
            .attr("href")
            .filter(|target| tag == "a" && target.starts_with('#'))
        {
            RunKind::Link(target.to_owned())
        } else if has_class(element, "rubrica") {
            RunKind::Rubric
        } else if has_class(element, "citazione") || tag == "small" {
            RunKind::Small
        } else if matches!(tag, "em" | "i") {
            RunKind::Italic
        } else {
            inherited.clone()
        };
        for child in element.children() {
            self.visit(child, &kind, depth + 1)?;
        }
        if is_block {
            self.finish_block();
        }
        Ok(())
    }

    fn anchor(&mut self, name: &str) {
        if !name.is_empty() {
            self.finish_block();
            if !self.anchors.iter().any(|anchor| anchor == name) {
                self.anchors.push(name.to_owned());
            }
        }
    }

    fn text(&mut self, text: &str, kind: &RunKind) {
        for ch in text.chars() {
            if ch.is_ascii_whitespace() {
                let last = self
                    .runs
                    .last()
                    .and_then(|run| run.text.chars().next_back());
                if last.is_some_and(|ch| !ch.is_ascii_whitespace()) {
                    self.push(' ', kind);
                }
            } else {
                self.consecutive_breaks = 0;
                self.push(ch, kind);
            }
        }
    }

    fn push(&mut self, ch: char, kind: &RunKind) {
        if let Some(last) = self.runs.last_mut().filter(|run| run.kind == *kind) {
            last.text.push(ch);
        } else {
            self.runs.push(Run {
                text: ch.to_string(),
                kind: kind.clone(),
            });
        }
    }

    fn line_break(&mut self, kind: &RunKind) {
        if self.runs.is_empty() {
            return;
        }
        self.trim_end();
        self.consecutive_breaks += 1;
        if self.consecutive_breaks == 2 {
            self.finish_block();
        } else {
            self.push('\n', kind);
        }
    }

    fn trim_end(&mut self) {
        while let Some(run) = self.runs.last_mut() {
            run.text.truncate(
                run.text
                    .trim_end_matches(|ch: char| ch.is_ascii_whitespace())
                    .len(),
            );
            if run.text.is_empty() {
                self.runs.pop();
            } else {
                break;
            }
        }
    }

    fn finish_block(&mut self) {
        self.trim_end();
        self.consecutive_breaks = 0;
        if self.runs.is_empty() {
            return;
        }
        let text: String = self.runs.iter().map(|run| run.text.as_str()).collect();
        let occurrence = self.occurrences.entry(text.clone()).or_default();
        let block = Block {
            id: Block::id_for(&text, *occurrence),
            anchors: std::mem::take(&mut self.anchors),
            runs: std::mem::take(&mut self.runs),
        };
        *occurrence += 1;
        if self.sections.is_empty() {
            self.sections.push(Section {
                title: "Opening".to_owned(),
                blocks: Vec::new(),
            });
        }
        self.sections.last_mut().unwrap().blocks.push(block);
    }
}

fn has_class(element: ElementRef<'_>, class: &str) -> bool {
    element.value().classes().any(|value| value == class)
}

fn is_site_navigation(element: ElementRef<'_>) -> bool {
    if matches!(element.attr("id"), Some("menu" | "menu_bar"))
        || element.attr("href") == Some("#menu")
    {
        return true;
    }
    if element.value().name() != "p" {
        return false;
    }
    let text = element_text(element);
    if !text.is_empty() && text.chars().all(|ch| ch == '*') {
        return true;
    }
    element.select(&selector("a[href]")).any(|link| {
        let href = link.attr("href").unwrap().to_ascii_lowercase();
        href == "http://www.ibreviary.com/new/donazione.html"
            || href == "https://www.ibreviary.com/new/donazione.html"
            || href == "http://www.ibreviary.com/new/newsletter.html"
            || href == "https://www.ibreviary.com/new/newsletter.html"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFICE: &str = include_str!("../tests/fixtures/provider-office.html");
    const CALENDAR: &str = include_str!("../tests/fixtures/provider-calendar.html");

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
    }

    fn parse(html: &str) -> Result<Office> {
        parse_office(
            html,
            date(),
            Hour::Night,
            "Example".into(),
            "https://example.test/office".into(),
        )
    }

    #[test]
    fn preserves_order_line_breaks_alternatives_and_credits() {
        let office = parse(OFFICE).unwrap();
        assert_eq!(office.date, date());
        assert_eq!(office.hour, Hour::Night);
        assert_eq!(office.celebration, "Example");
        assert_eq!(office.source_url, "https://example.test/office");
        assert_eq!(
            office
                .sections
                .iter()
                .map(|s| s.title.as_str())
                .collect::<Vec<_>>(),
            ["INTRODUCTION", "HYMN", "PSALMODY"]
        );
        assert_eq!(
            office
                .sections
                .iter()
                .flat_map(|s| &s.blocks)
                .map(Block::text)
                .collect::<Vec<_>>(),
            [
                "First synthetic line with a red instruction.\nSecond synthetic line, with an italic aside and & an entity.",
                "A separate synthetic paragraph finishes this introduction without borrowing prayer text.",
                "Choose the alternative",
                "These invented verses exercise the main text and retain their printed order.\nAnother invented verse belongs on the following line.",
                "Synthetic credit © 2099 Example Author\nSecond credit line",
                "This synthetic paragraph has enough text to represent a complete section in the parser fixture.",
                "Or:",
                "An invented alternative remains after the promotions and before its copyright.",
                "Alternative copyright © 2099 Example Author",
                "Return to the main section",
            ]
        );
    }

    #[test]
    fn preserves_styles_and_resolves_named_anchors_to_blocks() {
        let office = parse(OFFICE).unwrap();
        let intro = &office.sections[0].blocks[0];
        assert!(
            intro
                .runs
                .iter()
                .any(|r| r.kind == RunKind::Rubric && r.text == "a red instruction")
        );
        assert!(
            intro
                .runs
                .iter()
                .any(|r| r.kind == RunKind::Italic && r.text == "an italic aside")
        );
        assert!(
            office.sections[1].blocks[2]
                .runs
                .iter()
                .all(|r| r.kind == RunKind::Small)
        );
        let link = &office.sections[1].blocks[0].runs;
        assert_eq!(link.len(), 1);
        assert_eq!(link[0].kind, RunKind::Link("#alternative".into()));
        assert_eq!(office.sections[2].blocks[0].anchors, ["psalm"]);
        assert_eq!(office.sections[2].blocks[1].anchors, ["alternative"]);
    }

    #[test]
    fn preserves_ids_and_anchors_on_headings_and_empty_elements() {
        let html = OFFICE.replace(
            "<a name=\"psalm\"></a><span class=\"capolettera_piccolo\">PSALMODY</span>",
            "<a name=\"psalm\"></a><span class=\"capolettera_piccolo\" id=\"heading-id\"><a name=\"heading-name\"></a>PSALMODY</span>",
        );
        let office = parse(&html).unwrap();
        assert_eq!(
            office.sections[2].blocks[0].anchors,
            ["psalm", "heading-id", "heading-name"]
        );
    }

    #[test]
    fn preserves_trailing_anchor_targets() {
        let html = OFFICE
            .replace("<hr>", "<a href='#end'>Go to the end</a>")
            .replace("</div></div>", "<a name='end'></a></div></div>");
        let office = parse(&html).unwrap();
        let last = office.sections.last().unwrap().blocks.last().unwrap();
        assert_eq!(last.anchors, ["end"]);
        assert_eq!(last.text(), "Return to the main section");
    }

    #[test]
    fn paragraph_ids_are_stable_and_distinguish_repeated_text() {
        let html = OFFICE.replace(
            "<hr>",
            "<p>Repeated synthetic paragraph.</p><p>Repeated synthetic paragraph.</p>",
        );
        let first = parse(&html).unwrap();
        let second = parse(&html).unwrap();
        let ids = |office: &Office| {
            office
                .sections
                .iter()
                .flat_map(|s| &s.blocks)
                .map(|b| b.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&first), ids(&second));
        let repeated: Vec<_> = first
            .sections
            .iter()
            .flat_map(|s| &s.blocks)
            .filter(|b| b.text() == "Repeated synthetic paragraph.")
            .collect();
        assert_eq!(repeated.len(), 2);
        assert_ne!(repeated[0].id, repeated[1].id);
        assert_eq!(
            repeated[0].id,
            Block::id_for("Repeated synthetic paragraph.", 0)
        );
        assert_eq!(
            repeated[1].id,
            Block::id_for("Repeated synthetic paragraph.", 1)
        );
    }

    #[test]
    fn verifies_calendar_date_and_returns_complete_celebration() {
        assert_eq!(
            parse_calendar(CALENDAR, date()).unwrap(),
            "Invented Example Celebration"
        );
        let error = parse_calendar(CALENDAR, date().succ_opt().unwrap())
            .unwrap_err()
            .to_string();
        assert!(error.contains("2026-10-22"));
        assert!(error.contains("2026-10-23"));
        assert!(parse_calendar(&CALENDAR.replace("Thursday, 22", "Friday, 22"), date()).is_err());
    }

    #[test]
    fn rejects_missing_invalid_and_non_english_calendar_metadata() {
        for html in [
            CALENDAR.replace("Thursday, 22 October 2026", "Giovedì, 22 Ottobre 2026"),
            CALENDAR.replace("Thursday, 22 October 2026", "Thursday, 32 October 2026"),
            CALENDAR.replace("<p>Thursday, 22 October 2026</p>", ""),
            CALENDAR.replace("Invented <em>Example</em> Celebration", ""),
            CALENDAR.replace("<p>Invented <em>Example</em> Celebration</p>", ""),
            CALENDAR.replace("<h1>Breviary</h1>", "<h1>Breviario</h1>"),
        ] {
            assert!(parse_calendar(&html, date()).is_err());
        }
    }

    #[test]
    fn requires_the_exact_english_hour_including_plural_daytime() {
        for hour in Hour::ALL {
            let title = if hour == Hour::Daytime {
                "Daytime Prayers"
            } else {
                hour.title()
            };
            let html = OFFICE.replace("Night Prayer", title);
            let office =
                parse_office(&html, date(), hour, "Example".into(), "example".into()).unwrap();
            assert_eq!(office.hour, hour);
        }
        let singular = OFFICE.replace("Night Prayer", "Daytime Prayer");
        assert!(
            parse_office(
                &singular,
                date(),
                Hour::Daytime,
                "Example".into(),
                "example".into()
            )
            .is_err()
        );
        for html in [
            OFFICE.replace("Night Prayer", "Evening Prayer"),
            OFFICE.replace("Night Prayer", "Compieta"),
            OFFICE.replace("<h1>Breviary</h1>", "<h1>Breviario</h1>"),
            OFFICE.replace("<span class=\"sezione\">Night Prayer</span>", ""),
            OFFICE.replace("<hr>", "<span class=\"sezione\">Night Prayer</span>"),
        ] {
            assert!(parse(&html).is_err());
        }
    }

    #[test]
    fn rejects_truncated_malformed_and_missing_content() {
        for html in [
            OFFICE[..OFFICE.find("An invented alternative").unwrap()].to_owned(),
            OFFICE.replace("</body>", ""),
            OFFICE.replace("</html>", ""),
            OFFICE.replace("id=\"contenuto\"", "id=\"wrong-container\""),
            OFFICE.replace("<hr>", "<span>unclosed element"),
            OFFICE.replace("<a name=\"alternative\"></a>", ""),
            OFFICE.replace("capolettera_piccolo", "unknown-heading"),
        ] {
            assert!(parse(&html).is_err());
        }
    }

    #[test]
    fn rejects_empty_headings_and_missing_prayer_body() {
        let empty_heading = OFFICE.replace(
            "<span class=\"capolettera_piccolo\">INTRODUCTION</span>",
            "<span class=\"capolettera_piccolo\"></span>",
        );
        assert!(parse(&empty_heading).is_err());
        let empty_section = OFFICE.replace(
            "</div></div>",
            "<span class=\"capolettera_piccolo\">EMPTY</span></div></div>",
        );
        assert!(parse(&empty_section).is_err());
        let no_body = "<!DOCTYPE html><html><body><div id='contenuto'><div class='inner'><h1>Breviary</h1><span class='sezione'>Night Prayer</span><span class='capolettera_piccolo'>ONE</span><p>A</p><span class='capolettera_piccolo'>TWO</span><p>B</p></div></div></body></html>";
        assert!(parse(no_body).is_err());
    }

    #[test]
    fn tolerates_the_sources_known_unclosed_menu_without_accepting_other_damage() {
        let html = OFFICE.replace(
            "<body>",
            "<body><div id='menu_bar'><ul><li>Menu</li><ul></div>",
        );
        assert!(parse(&html).is_ok());
        assert!(parse(&html.replace("<hr>", "<span>unclosed element")).is_err());
        let valid_menu = OFFICE.replace(
            "<body>",
            "<body><div id='menu_bar'><ul><li>Menu</li></ul></div>",
        );
        assert!(parse(&valid_menu.replace("<hr>", "<span>unclosed element")).is_err());
    }

    #[test]
    fn cancellation_and_empty_requests_do_not_start_downloads() {
        let cancelled = AtomicBool::new(true);
        let mut emitted = false;
        let error = download_date(date(), &Hour::ALL, &cancelled, |_| emitted = true).unwrap_err();
        assert_eq!(error.to_string(), "Download cancelled.");
        assert!(!emitted);
        download_date(date(), &[], &AtomicBool::new(false), |_| {
            panic!("empty request emitted an office")
        })
        .unwrap();
    }
}
