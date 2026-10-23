#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{Context, Result, bail};
use chrono::{Days, Local, NaiveDate};
use hours_rs::{
    app::{HoursApp, Startup},
    model::Hour,
    provider,
    store::Library,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("Hours: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--help" || a == "-h") {
        println!(
            "Hours · A native Liturgy of the Hours reader\n\n  hours-rs                            Open the desktop app\n  hours-rs --offline                  Open with downloads disabled\n  hours-rs --date YYYY-MM-DD --hour morning\n  hours-rs download YYYY-MM-DD [DAYS] Download all hours for 1–30 days\n  hours-rs read YYYY-MM-DD HOUR       Read a downloaded office\n  hours-rs list                       List downloaded dates\n  hours-rs export PATH                Export a library backup\n  hours-rs import PATH                Import a library backup\n\nHours: readings, morning, daytime, evening, night\nHOURS_RS_HOME overrides the local data directory.\n--screenshot PATH captures the native window and exits."
        );
        return Ok(());
    }
    let library = Library::open(&Library::default_path()?)?;
    match args.first().map(String::as_str) {
        Some("download") => {
            let date = parse_date(
                args.get(1)
                    .context("Provide a start date, for example: download 2026-10-22 7")?,
            )?;
            let days: u32 = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(1);
            if !(1..=30).contains(&days) {
                bail!("Choose between 1 and 30 days.");
            }
            let cancelled = AtomicBool::new(false);
            for day in 0..days {
                let date = date
                    .checked_add_days(Days::new(u64::from(day)))
                    .context("Date is outside the supported range")?;
                let existing = library.availability(date)?;
                let hours: Vec<_> = Hour::ALL
                    .into_iter()
                    .filter(|h| !existing.contains(h))
                    .collect();
                if hours.is_empty() {
                    println!("{date}: already downloaded");
                    continue;
                }
                let mut storage_error = None;
                provider::download_date(date, &hours, &cancelled, |office| {
                    match library.save_office(&office) {
                        Ok(_) => println!("Saved {} · {}", office.date, office.hour.title()),
                        Err(error) => {
                            cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                            storage_error = Some(error);
                        }
                    }
                })?;
                if let Some(error) = storage_error {
                    return Err(error);
                }
            }
            return Ok(());
        }
        Some("read") => {
            let date = parse_date(args.get(1).context("Provide YYYY-MM-DD and an hour")?)?;
            let hour = parse_hour(
                args.get(2)
                    .context("Provide an hour: readings, morning, daytime, evening, or night")?,
            )?;
            let office = library.office(date, hour)?.context(
                "This hour is not downloaded. Use the download command while connected.",
            )?;
            println!("{}", office.text());
            return Ok(());
        }
        Some("list") => {
            for (date, count) in library.downloaded_dates()? {
                println!("{date}  {count}/5 hours");
            }
            return Ok(());
        }
        Some("export") => {
            library.export_to(Path::new(args.get(1).context("Provide a backup path")?))?;
            println!("Library exported");
            return Ok(());
        }
        Some("import") => {
            let count =
                library.import_from(Path::new(args.get(1).context("Provide a backup path")?))?;
            println!("Imported {count} offices");
            return Ok(());
        }
        _ => {}
    }
    let mut startup = Startup::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--offline" => startup.offline = true,
            "--date" => {
                index += 1;
                startup.date = Some(parse_date(
                    args.get(index).context("--date requires YYYY-MM-DD")?,
                )?);
            }
            "--hour" => {
                index += 1;
                startup.hour = Some(parse_hour(
                    args.get(index).context("--hour requires an hour name")?,
                )?);
            }
            "--screenshot" => {
                index += 1;
                startup.screenshot = Some(PathBuf::from(
                    args.get(index)
                        .context("--screenshot requires a PNG path")?,
                ));
            }
            arg if arg.starts_with("-psn_") => {}
            arg => bail!("Unknown option {arg:?}. Run hours-rs --help."),
        }
        index += 1;
    }
    let icon = image::load_from_memory(include_bytes!("../assets/app-icon.png"))?.into_rgba8();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1220.0, 880.0])
            .with_min_inner_size([760.0, 560.0])
            .with_app_id("com.chrisvo.hours-rs")
            .with_icon(Arc::new(eframe::egui::IconData {
                width: icon.width(),
                height: icon.height(),
                rgba: icon.into_raw(),
            })),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Hours",
        options,
        Box::new(move |cc| Ok(Box::new(HoursApp::new(cc, library, startup)))),
    )
    .map_err(|e| anyhow::anyhow!("Unable to open the desktop window: {e}"))?;
    Ok(())
}

fn parse_date(text: &str) -> Result<NaiveDate> {
    if text == "today" {
        return Ok(Local::now().date_naive());
    }
    let date =
        NaiveDate::parse_from_str(text, "%Y-%m-%d").context("Use a date in YYYY-MM-DD format.")?;
    hours_rs::model::validate_date(date)?;
    Ok(date)
}

fn parse_hour(text: &str) -> Result<Hour> {
    Hour::ALL
        .into_iter()
        .find(|h| h.key() == text)
        .context("Use readings, morning, daytime, evening, or night.")
}
