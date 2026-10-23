use crate::{
    icons::{self, Icon},
    model::{Hour, Office, RunKind},
    provider,
    store::{Annotation, Library},
    theme::{self, Theme},
};
use chrono::{Datelike, Days, Local, Months, NaiveDate};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, Key, Layout, Margin, RichText, ScrollArea, Sense,
    Stroke, TextEdit, Vec2,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    theme: Theme,
    font_size: f32,
    line_spacing: f32,
    width: f32,
    sidebar: bool,
    offline: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: Theme::Parchment,
            font_size: 20.0,
            line_spacing: 1.65,
            width: 650.0,
            sidebar: true,
            offline: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Panel {
    None,
    Search,
    Journal,
}

enum DownloadEvent {
    Office(Office),
    Progress(String),
    Finished(Result<(), String>),
}

struct Download {
    receiver: mpsc::Receiver<DownloadEvent>,
    cancelled: Arc<AtomicBool>,
    message: String,
    saved: usize,
    total: usize,
}

#[derive(Default)]
pub struct Startup {
    pub date: Option<NaiveDate>,
    pub hour: Option<Hour>,
    pub offline: bool,
    pub screenshot: Option<PathBuf>,
}

struct NoteDraft {
    anchor: Annotation,
    text: String,
    dirty: bool,
}

pub struct HoursApp {
    library: Library,
    preferences: Preferences,
    saved_preferences: String,
    date: NaiveDate,
    hour: Hour,
    office: Option<Arc<Office>>,
    available: Vec<Hour>,
    completed: Vec<Hour>,
    marks: HashMap<String, Annotation>,
    panel: Panel,
    query: String,
    search_focus: bool,
    settings_open: bool,
    calendar_open: bool,
    calendar_month: NaiveDate,
    date_input: String,
    library_open: bool,
    help_open: bool,
    download_days: u32,
    download: Option<Download>,
    transfer_path: String,
    notice: Option<(String, Instant)>,
    error: Option<String>,
    jump: Option<String>,
    selected: Option<String>,
    note: Option<NoteDraft>,
    scroll_offset: f32,
    restore_offset: Option<f32>,
    active_section: usize,
    focus: bool,
    screenshot: Option<PathBuf>,
    frames: u32,
}

impl HoursApp {
    pub fn new(cc: &eframe::CreationContext<'_>, library: Library, startup: Startup) -> Self {
        Self::with_context(&cc.egui_ctx, library, startup)
    }

    fn with_context(ctx: &egui::Context, library: Library, startup: Startup) -> Self {
        theme::install_fonts(ctx);
        let mut preferences = library
            .setting("preferences")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str::<Preferences>(&s).ok())
            .unwrap_or_default();
        preferences.font_size = preferences.font_size.clamp(15.0, 30.0);
        preferences.line_spacing = preferences.line_spacing.clamp(1.25, 2.0);
        preferences.width = preferences.width.clamp(460.0, 850.0);
        if startup.offline {
            preferences.offline = true;
        }
        preferences.theme.apply(ctx);
        let date = startup.date.unwrap_or_else(|| Local::now().date_naive());
        let hour = startup.hour.unwrap_or_else(Hour::suggested);
        let mut app = Self {
            library,
            saved_preferences: String::new(),
            preferences,
            date,
            hour,
            office: None,
            available: Vec::new(),
            completed: Vec::new(),
            marks: HashMap::new(),
            panel: Panel::None,
            query: String::new(),
            search_focus: false,
            settings_open: false,
            calendar_open: false,
            calendar_month: date.with_day(1).unwrap(),
            date_input: date.to_string(),
            library_open: false,
            help_open: false,
            download_days: 7,
            download: None,
            transfer_path: String::new(),
            notice: None,
            error: None,
            jump: None,
            selected: None,
            note: None,
            scroll_offset: 0.0,
            restore_offset: None,
            active_section: 0,
            focus: false,
            screenshot: startup.screenshot,
            frames: 0,
        };
        app.load_office();
        app
    }

    fn fail(&mut self, error: impl std::fmt::Display) {
        self.error = Some(error.to_string());
    }

    fn tell(&mut self, message: impl Into<String>) {
        self.notice = Some((message.into(), Instant::now()));
    }

    fn load_office(&mut self) {
        self.office = match self.library.office(self.date, self.hour) {
            Ok(office) => office.map(Arc::new),
            Err(error) => {
                self.fail(error);
                None
            }
        };
        match self.library.availability(self.date) {
            Ok(v) => self.available = v,
            Err(e) => self.fail(e),
        }
        match self.library.completed(self.date) {
            Ok(v) => self.completed = v,
            Err(e) => self.fail(e),
        }
        match self.library.annotations() {
            Ok(marks) => {
                self.marks = marks
                    .into_iter()
                    .filter(|a| a.date == self.date && a.hour == self.hour)
                    .map(|a| (a.block_id.clone(), a))
                    .collect()
            }
            Err(e) => self.fail(e),
        }
        self.restore_offset = match self.library.position(self.date, self.hour) {
            Ok(v) => Some(v),
            Err(e) => {
                self.fail(e);
                Some(0.0)
            }
        };
        self.scroll_offset = self.restore_offset.unwrap_or_default();
        self.selected = None;
        self.active_section = 0;
    }

    fn navigate(&mut self, date: NaiveDate, hour: Hour) {
        if let Err(error) = crate::model::validate_date(date) {
            self.fail(error);
            return;
        }
        if date == self.date && hour == self.hour {
            return;
        }
        self.save_note();
        if self.note.as_ref().is_some_and(|n| n.dirty) {
            return;
        }
        self.note = None;
        self.persist_position();
        self.date = date;
        self.hour = hour;
        self.date_input = date.to_string();
        self.jump = None;
        self.load_office();
    }

    fn persist_position(&mut self) {
        if self.office.is_some()
            && let Err(e) = self
                .library
                .save_position(self.date, self.hour, self.scroll_offset)
        {
            self.fail(e);
        }
    }

    fn import_backup(&mut self, path: &std::path::Path) {
        match self.library.import_from(path) {
            Ok(count) => {
                let offset = self.office.as_ref().map(|_| self.scroll_offset);
                let selection = self.selected.clone();
                self.load_office();
                if let Some(offset) = offset {
                    self.restore_offset = Some(offset);
                }
                self.selected = selection;
                self.tell(format!("Imported {count} offices"));
            }
            Err(error) => self.fail(error),
        }
    }

    fn change_day(&mut self, delta: i64) {
        if let Some(date) = self.date.checked_add_signed(chrono::Duration::days(delta)) {
            self.navigate(date, self.hour);
        }
    }

    fn toggle_complete(&mut self) {
        if self.office.is_none() {
            return;
        }
        let complete = !self.completed.contains(&self.hour);
        match self.library.set_completed(self.date, self.hour, complete) {
            Ok(()) => {
                if complete {
                    self.completed.push(self.hour);
                } else {
                    self.completed.retain(|h| *h != self.hour);
                }
                self.tell(if complete {
                    "Marked as prayed"
                } else {
                    "Prayer mark removed"
                });
            }
            Err(e) => self.fail(e),
        }
    }

    fn save_mark(&mut self, mark: Annotation) {
        match self.library.save_annotation(&mark) {
            Ok(()) => {
                self.marks.insert(mark.block_id.clone(), mark);
            }
            Err(e) => self.fail(e),
        }
    }

    fn mark_for(&self, id: &str) -> Option<Annotation> {
        if let Some(mark) = self.marks.get(id) {
            return Some(mark.clone());
        }
        self.office.as_ref()?.block(id).map(|b| Annotation {
            date: self.date,
            hour: self.hour,
            block_id: id.into(),
            quote: b.text(),
            note: String::new(),
            bookmarked: false,
            highlighted: false,
        })
    }

    fn edit_note(&mut self, id: String) {
        self.save_note();
        if self.note.as_ref().is_some_and(|n| n.dirty) {
            return;
        }
        if let Some(anchor) = self.mark_for(&id) {
            self.note = Some(NoteDraft {
                text: anchor.note.clone(),
                anchor,
                dirty: false,
            });
            self.selected = Some(id);
        }
    }

    fn save_note(&mut self) {
        let Some(draft) = self.note.as_ref().filter(|draft| draft.dirty) else {
            return;
        };
        let mut mark = match self.library.annotation(
            draft.anchor.date,
            draft.anchor.hour,
            &draft.anchor.block_id,
        ) {
            Ok(Some(mark)) => mark,
            Ok(None) => Annotation {
                bookmarked: false,
                highlighted: false,
                ..draft.anchor.clone()
            },
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        mark.note = draft.text.clone();
        match self.library.save_annotation(&mark) {
            Ok(()) => {
                if mark.date == self.date && mark.hour == self.hour {
                    self.marks.insert(mark.block_id.clone(), mark);
                }
                self.note.as_mut().unwrap().dirty = false;
            }
            Err(error) => self.fail(error),
        }
    }

    fn start_download(&mut self, dates: Vec<NaiveDate>, only: Option<Hour>, ctx: &egui::Context) {
        if self.preferences.offline || self.download.is_some() {
            return;
        }
        let mut plan = Vec::new();
        for date in dates {
            let existing = match self.library.availability(date) {
                Ok(v) => v,
                Err(e) => {
                    self.fail(e);
                    return;
                }
            };
            let hours: Vec<_> = only
                .map(|h| vec![h])
                .unwrap_or_else(|| Hour::ALL.to_vec())
                .into_iter()
                .filter(|h| !existing.contains(h))
                .collect();
            if !hours.is_empty() {
                plan.push((date, hours));
            }
        }
        let total = plan.iter().map(|(_, hours)| hours.len()).sum();
        if total == 0 {
            self.tell("These prayers are already available offline");
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let mut result = Ok(());
            for (date, hours) in plan {
                if stop.load(Ordering::Relaxed) {
                    result = Err("Download stopped. Completed offices are saved.".into());
                    break;
                }
                let _ = sender.send(DownloadEvent::Progress(format!(
                    "Downloading {}",
                    date.format("%b %-d")
                )));
                repaint.request_repaint();
                if let Err(error) = provider::download_date(date, &hours, &stop, |office| {
                    let _ = sender.send(DownloadEvent::Office(office));
                    repaint.request_repaint();
                }) {
                    result = Err(format!("{error:#}"));
                    break;
                }
            }
            let _ = sender.send(DownloadEvent::Finished(result));
            repaint.request_repaint();
        });
        self.download = Some(Download {
            receiver,
            cancelled,
            message: "Connecting to iBreviary".into(),
            saved: 0,
            total,
        });
    }

    fn receive_downloads(&mut self) {
        let events = self
            .download
            .as_ref()
            .map(|d| d.receiver.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut reload = false;
        for event in events {
            match event {
                DownloadEvent::Office(office) => {
                    if self
                        .download
                        .as_ref()
                        .is_some_and(|d| d.cancelled.load(Ordering::Relaxed))
                    {
                        continue;
                    }
                    match self.library.save_office(&office) {
                        Ok(_) => {
                            if let Some(d) = &mut self.download {
                                d.saved += 1;
                                d.message = format!(
                                    "Saved {} · {}",
                                    office.date.format("%b %-d"),
                                    office.hour.title()
                                );
                            }
                            if office.date == self.date {
                                if !self.available.contains(&office.hour) {
                                    self.available.push(office.hour);
                                }
                                reload |= self.office.is_none() && office.hour == self.hour;
                            }
                        }
                        Err(e) => {
                            if let Some(d) = &self.download {
                                d.cancelled.store(true, Ordering::Relaxed);
                            }
                            self.fail(e);
                        }
                    }
                }
                DownloadEvent::Progress(message) => {
                    if let Some(d) = &mut self.download {
                        d.message = message;
                    }
                }
                DownloadEvent::Finished(result) => {
                    let saved = self.download.as_ref().map_or(0, |d| d.saved);
                    self.download = None;
                    match result {
                        Ok(()) => self.tell(format!("{saved} offices saved for offline prayer")),
                        Err(e) => self.fail(e),
                    }
                }
            }
        }
        if reload {
            self.load_office();
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let command = egui::Modifiers::COMMAND;
        if ctx.input_mut(|i| i.consume_key(command, Key::F)) {
            self.panel = Panel::Search;
            self.search_focus = true;
            self.focus = false;
        }
        if ctx.input_mut(|i| i.consume_key(command, Key::B)) {
            self.preferences.sidebar = !self.preferences.sidebar;
        }
        if ctx.input_mut(|i| i.consume_key(command, Key::Comma)) {
            self.settings_open = !self.settings_open;
        }
        if ctx.input_mut(|i| i.consume_key(command, Key::K)) {
            self.calendar_open = !self.calendar_open;
            self.calendar_month = self.date.with_day(1).unwrap();
        }
        if ctx.input_mut(|i| i.consume_key(command, Key::Enter)) {
            self.toggle_complete();
        }
        if ctx
            .input_mut(|i| i.consume_key(command, Key::Plus) || i.consume_key(command, Key::Equals))
        {
            self.preferences.font_size = (self.preferences.font_size + 1.0).min(30.0);
        }
        if ctx.input_mut(|i| i.consume_key(command, Key::Minus)) {
            self.preferences.font_size = (self.preferences.font_size - 1.0).max(15.0);
        }
        if ctx.input_mut(|i| i.consume_key(command, Key::Num0)) {
            self.preferences.font_size = 20.0;
        }
        if ctx.input_mut(|i| i.consume_key(command, Key::J)) {
            self.panel = if self.panel == Panel::Journal {
                Panel::None
            } else {
                Panel::Journal
            };
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.save_note();
            if self.note.as_ref().is_some_and(|n| n.dirty) {
                return;
            }
            self.note = None;
            self.calendar_open = false;
            self.settings_open = false;
            self.library_open = false;
            self.help_open = false;
            self.focus = false;
            self.panel = Panel::None;
        }
        for (key, hour) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5]
            .into_iter()
            .zip(Hour::ALL)
        {
            if ctx.input_mut(|i| i.consume_key(command, key)) {
                self.navigate(self.date, hour);
            }
        }
        if !ctx.wants_keyboard_input()
            && !self.calendar_open
            && !self.settings_open
            && !self.library_open
        {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowRight)) {
                self.navigate(self.date, self.hour.next());
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft)) {
                self.navigate(self.date, self.hour.previous());
            }
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        let p = self.preferences.theme.palette();
        egui::TopBottomPanel::top("toolbar")
            .exact_height(54.0)
            .frame(
                Frame::new()
                    .fill(p.page)
                    .inner_margin(Margin::symmetric(14, 11))
                    .stroke(Stroke::new(0.5_f32, p.line)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if icons::button(ui, Icon::Sidebar, "Toggle hours sidebar (⌘B)", p.muted)
                        .clicked()
                    {
                        self.preferences.sidebar = !self.preferences.sidebar;
                        self.focus = false;
                    }
                    ui.add_space(12.0);
                    if icons::button(ui, Icon::Left, "Previous day", p.muted).clicked() {
                        self.change_day(-1);
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(self.date.format("%A, %B %-d").to_string())
                                    .size(13.0),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        self.calendar_open = !self.calendar_open;
                        self.calendar_month = self.date.with_day(1).unwrap();
                    }
                    if icons::button(ui, Icon::Right, "Next day", p.muted).clicked() {
                        self.change_day(1);
                    }
                    if self.date != Local::now().date_naive() && ui.small_button("Today").clicked()
                    {
                        self.navigate(Local::now().date_naive(), self.hour);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icons::button(ui, Icon::Settings, "Reading appearance (⌘,)", p.muted)
                            .clicked()
                        {
                            self.settings_open = !self.settings_open;
                        }
                        if icons::button(ui, Icon::Focus, "Focus on prayer", p.muted).clicked() {
                            self.focus = !self.focus;
                        }
                        if icons::button(ui, Icon::Note, "Notes & bookmarks (⌘J)", p.muted)
                            .clicked()
                        {
                            self.panel = if self.panel == Panel::Journal {
                                Panel::None
                            } else {
                                Panel::Journal
                            };
                            self.focus = false;
                        }
                        if icons::button(ui, Icon::Search, "Search this hour (⌘F)", p.muted)
                            .clicked()
                        {
                            self.panel = if self.panel == Panel::Search {
                                Panel::None
                            } else {
                                self.search_focus = true;
                                Panel::Search
                            };
                            self.focus = false;
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let p = self.preferences.theme.palette();
        egui::TopBottomPanel::bottom("status")
            .exact_height(32.0)
            .frame(
                Frame::new()
                    .fill(p.page)
                    .inner_margin(Margin::symmetric(20, 8))
                    .stroke(Stroke::new(0.5_f32, p.line)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let color = if self.office.is_some() {
                        p.accent
                    } else {
                        p.faint
                    };
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 2.5, color);
                    ui.label(
                        RichText::new(if self.office.is_some() {
                            "Available offline"
                        } else {
                            "Not downloaded"
                        })
                        .size(10.0)
                        .color(p.muted),
                    );
                    ui.label(
                        RichText::new("·  Roman office  ·  English")
                            .size(10.0)
                            .color(p.faint),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(d) = &self.download {
                            ui.label(
                                RichText::new(format!("{}   {}/{}", d.message, d.saved, d.total))
                                    .size(10.0)
                                    .color(p.accent),
                            );
                        } else if let Some((message, at)) = &self.notice {
                            if at.elapsed() < Duration::from_secs(8) {
                                ui.label(RichText::new(message).size(10.0).color(p.accent));
                            }
                        } else {
                            ui.label(
                                RichText::new("Pax et bonum")
                                    .font(theme::heading(10.0))
                                    .color(p.faint),
                            );
                        }
                    });
                });
            });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        if self.focus
            || !self.preferences.sidebar
            || (self.panel != Panel::None && ctx.content_rect().width() < 1100.0)
        {
            return;
        }
        let p = self.preferences.theme.palette();
        egui::SidePanel::left("hours")
            .exact_width(258.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(p.sidebar)
                    .inner_margin(Margin::symmetric(18, 24))
                    .stroke(Stroke::new(0.5_f32, p.line)),
            )
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("sidebar_footer")
                    .exact_height(76.0)
                    .frame(Frame::NONE)
                    .show_inside(ui, |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("How to begin").size(11.0).color(p.muted),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            self.help_open = true;
                        }
                        ui.horizontal(|ui| {
                            if icons::button(ui, Icon::Download, "Offline library", p.accent)
                                .clicked()
                            {
                                self.library_open = true;
                            }
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("Offline library").size(12.0))
                                        .frame(false),
                                )
                                .clicked()
                            {
                                self.library_open = true;
                            }
                            ui.label(
                                RichText::new(format!("{}/5", self.available.len()))
                                    .size(10.0)
                                    .color(p.faint),
                            );
                        });
                        ui.separator();
                    });
                ScrollArea::vertical()
                    .id_salt("sidebar_body")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                            ui.painter().circle_stroke(
                                r.center(),
                                16.0,
                                Stroke::new(0.8_f32, p.faint),
                            );
                            icons::draw(ui.painter(), r.shrink(7.0), Icon::Cross, p.accent);
                            ui.add_space(5.0);
                            ui.label(
                                RichText::new("Hours")
                                    .font(theme::heading(27.0))
                                    .color(p.text),
                            );
                        });
                        ui.add_space(7.0);
                        ui.label(
                            RichText::new("THE LITURGY OF THE HOURS")
                                .size(8.5)
                                .extra_letter_spacing(1.15)
                                .color(p.muted),
                        );
                        ui.add_space(29.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("THE DAILY OFFICE")
                                    .size(9.0)
                                    .extra_letter_spacing(1.4)
                                    .color(p.muted),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("{} / 5", self.completed.len()))
                                        .size(10.0)
                                        .color(p.faint),
                                );
                            });
                        });
                        ui.add_space(7.0);
                        for hour in Hour::ALL {
                            let selected = hour == self.hour;
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 57.0),
                                Sense::click(),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::SelectableLabel,
                                    true,
                                    hour.title(),
                                )
                            });
                            if selected || response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    5.0,
                                    if selected {
                                        p.active
                                    } else {
                                        p.active.gamma_multiply(0.45)
                                    },
                                );
                            }
                            if selected {
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        rect.left_top() + egui::vec2(0.0, 12.0),
                                        egui::vec2(2.0, 33.0),
                                    ),
                                    1.0,
                                    p.accent,
                                );
                            }
                            let icon = match hour {
                                Hour::Readings => Icon::Book,
                                Hour::Morning => Icon::Sunrise,
                                Hour::Daytime => Icon::Sun,
                                Hour::Evening => Icon::Sunset,
                                Hour::Night => Icon::Moon,
                            };
                            icons::draw(
                                ui.painter(),
                                egui::Rect::from_min_size(
                                    rect.left_top() + egui::vec2(12.0, 17.0),
                                    Vec2::splat(20.0),
                                ),
                                icon,
                                if selected { p.accent } else { p.muted },
                            );
                            ui.painter().text(
                                rect.left_top() + egui::vec2(43.0, 13.0),
                                Align2::LEFT_TOP,
                                hour.title(),
                                theme::heading(if hour == Hour::Readings { 12.0 } else { 13.0 }),
                                if selected { p.accent } else { p.text },
                            );
                            let sub = match hour {
                                Hour::Readings => "At any time",
                                Hour::Morning => "Lauds",
                                Hour::Daytime => "Terce · Sext · None",
                                Hour::Evening => "Vespers",
                                Hour::Night => "Compline",
                            };
                            ui.painter().text(
                                rect.left_top() + egui::vec2(43.0, 35.0),
                                Align2::LEFT_TOP,
                                sub,
                                FontId::proportional(10.0),
                                p.muted,
                            );
                            if self.completed.contains(&hour) {
                                icons::draw(
                                    ui.painter(),
                                    egui::Rect::from_center_size(
                                        rect.right_center() - egui::vec2(13.0, 0.0),
                                        Vec2::splat(13.0),
                                    ),
                                    Icon::Check,
                                    p.accent,
                                );
                            } else if self.available.contains(&hour) {
                                ui.painter().circle_filled(
                                    rect.right_center() - egui::vec2(12.0, 0.0),
                                    2.5,
                                    p.faint,
                                );
                            }
                            if response.clicked() {
                                self.navigate(self.date, hour);
                            }
                        }
                        ui.add_space(22.0);
                        ui.separator();
                        ui.add_space(13.0);
                        if let Some(office) = self.office.clone() {
                            ui.label(
                                RichText::new("IN THIS HOUR")
                                    .size(9.0)
                                    .extra_letter_spacing(1.4)
                                    .color(p.muted),
                            );
                            ui.add_space(7.0);
                            for (index, section) in office.sections.iter().enumerate() {
                                let text = RichText::new(&section.title).size(11.0).color(
                                    if index == self.active_section {
                                        p.accent
                                    } else {
                                        p.muted
                                    },
                                );
                                if ui
                                    .add(
                                        egui::Button::new(text)
                                            .frame(false)
                                            .min_size(egui::vec2(ui.available_width(), 24.0)),
                                    )
                                    .clicked()
                                {
                                    self.jump = Some(format!("section-{index}"));
                                }
                            }
                        } else {
                            ui.label(
                                RichText::new("A rhythm of prayer,\nthroughout the day.")
                                    .font(theme::reading(14.0))
                                    .color(p.muted),
                            );
                        }
                    });
            });
    }

    fn right_panel(&mut self, ctx: &egui::Context) {
        if self.panel == Panel::None || self.focus {
            return;
        }
        let p = self.preferences.theme.palette();
        egui::SidePanel::right("tools").exact_width(285.0).resizable(false).frame(Frame::new().fill(p.sidebar).inner_margin(Margin::same(18)).stroke(Stroke::new(0.5_f32,p.line))).show(ctx,|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(if self.panel==Panel::Search { "Find in prayer" } else { "Your journal" }).font(theme::heading(16.0)));
                ui.with_layout(Layout::right_to_left(Align::Center),|ui| { if icons::button(ui,Icon::Close,"Close panel",p.muted).clicked() { self.panel=Panel::None; } });
            });
            ui.add_space(14.0);
            if self.panel==Panel::Search {
                let field=ui.add(TextEdit::singleline(&mut self.query).hint_text("Search this hour…").desired_width(f32::INFINITY));
                if self.search_focus { field.request_focus(); self.search_focus=false; }
                if let Some(office)=self.office.clone() {
                    let results=office.search(&self.query);
                    ui.label(RichText::new(format!("{} passages",results.len())).small().color(p.muted));
                    ScrollArea::vertical().id_salt("results").show(ui,|ui| {
                        for (section,block) in results {
                            ui.add_space(8.0);
                            ui.label(RichText::new(&office.sections[section].title).size(10.0).color(p.accent));
                            let excerpt=truncate(&block.text(),175);
                            if ui.add(egui::Button::new(RichText::new(excerpt).size(12.0)).wrap().frame(false)).clicked() { self.jump=Some(block.id.clone()); self.selected=Some(block.id.clone()); }
                            ui.separator();
                        }
                    });
                } else { ui.label(RichText::new("Download this hour to search it offline.").small().color(p.muted)); }
            } else {
                ui.label(RichText::new("Notes and bookmarks stay on this device.").size(11.0).color(p.muted));
                let marks=match self.library.annotations() { Ok(v)=>v,Err(e)=> { self.fail(e); Vec::new() } };
                if marks.is_empty() { ui.add_space(25.0); ui.label(RichText::new("Right-click a passage to save a bookmark, highlight it, or write a note.").size(13.0).color(p.muted)); }
                ScrollArea::vertical().id_salt("journal").show(ui,|ui| {
                    for mark in marks.iter().filter(|m| m.bookmarked || m.highlighted || !m.note.is_empty()) {
                        ui.add_space(14.0);
                        ui.label(RichText::new(format!("{} · {}",mark.date.format("%b %-d"),mark.hour.title())).size(10.0).color(p.accent));
                        if ui.add(egui::Button::new(RichText::new(truncate(&mark.quote,140)).font(theme::reading(13.0))).wrap().frame(false)).clicked() {
                            self.navigate(mark.date,mark.hour); self.jump=Some(mark.block_id.clone()); self.selected=Some(mark.block_id.clone());
                        }
                        if !mark.note.is_empty() { ui.label(RichText::new(&mark.note).size(12.0).color(p.muted)); }
                        ui.separator();
                    }
                });
            }
        });
    }

    fn reader(&mut self, ctx: &egui::Context) {
        let p = self.preferences.theme.palette();
        egui::CentralPanel::default().frame(Frame::new().fill(p.page).inner_margin(Margin::ZERO)).show(ctx,|ui| {
            let width=self.preferences.width.min((ui.available_width()-60.0).max(220.0));
            let inset=((ui.available_width()-width)/2.0).max(18.0);
            let Some(office)=self.office.clone() else {
                ui.add_space((ui.available_height()*0.18).max(40.0));
                ui.vertical_centered(|ui| {
                    let (rect,_)=ui.allocate_exact_size(Vec2::splat(56.0),Sense::hover());
                    icons::draw(ui.painter(),rect.shrink(9.0),Icon::Book,p.accent);
                    ui.add_space(20.0);
                    ui.label(RichText::new(self.hour.title()).font(theme::heading(29.0)));
                    ui.add_space(7.0);
                    ui.label(RichText::new(self.date.format("%A, %B %-d, %Y").to_string()).size(13.0).color(p.muted));
                    ui.add_space(30.0);
                    ui.label(RichText::new("Prepare a little space for prayer.").font(theme::reading(19.0)).color(p.text));
                    ui.add_space(10.0);
                    ui.label(RichText::new("Download this office once, then read it here offline.").size(13.0).color(p.muted));
                    ui.add_space(25.0);
                    if let Some(d)=&self.download { ui.spinner(); ui.label(RichText::new(&d.message).size(12.0).color(p.muted)); }
                    else if self.preferences.offline {
                        ui.label(RichText::new("Offline mode is on. This hour is not in your library yet.").size(12.0).color(p.accent));
                        if ui.button("Open offline library").clicked() { self.library_open=true; }
                    } else {
                        if ui.add(egui::Button::new(RichText::new("Download this hour").color(p.page)).fill(p.accent).min_size(egui::vec2(190.0,40.0))).clicked() { self.start_download(vec![self.date],Some(self.hour),ctx); }
                        if ui.add(egui::Button::new(RichText::new("Download all hours for this day").size(12.0).color(p.muted)).frame(false)).clicked() { self.start_download(vec![self.date],None,ctx); }
                    }
                    ui.add_space(38.0);
                    ui.label(RichText::new("Modern Roman office · English texts from iBreviary").size(10.0).color(p.faint));
                });
                return;
            };
            let mut scroll=ScrollArea::vertical().id_salt(("reader",self.date,self.hour)).auto_shrink([false,false]);
            if let Some(offset)=self.restore_offset.take() { scroll=scroll.vertical_scroll_offset(offset); }
            let output=scroll.show(ui,|ui| {
                ui.horizontal_top(|ui| {
                    ui.add_space(inset);
                    ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.add_space(53.0);
                    ui.label(RichText::new(self.date.format("%A · %-d %B %Y").to_string().to_uppercase()).size(9.5).extra_letter_spacing(1.6).color(p.accent));
                    ui.add_space(18.0);
                    ui.label(RichText::new(office.hour.title()).font(theme::heading(33.0)).color(p.text));
                    ui.add_space(7.0);
                    ui.label(RichText::new(&office.celebration).font(theme::reading(13.0)).color(p.muted));
                    ui.add_space(10.0);
                    ui.label(RichText::new(office.hour.subtitle()).size(10.5).color(p.faint));
                    ui.add_space(27.0);
                    let (line,_)=ui.allocate_exact_size(egui::vec2(width,1.0),Sense::hover());
                    ui.painter().line_segment([line.left_center(),line.right_center()],Stroke::new(0.7_f32,p.line));
                    ui.add_space(13.0);
                    for (section_index,section) in office.sections.iter().enumerate() {
                        ui.add_space(24.0);
                        let heading=ui.label(RichText::new(&section.title).font(theme::heading(14.0)).color(p.accent));
                        if heading.rect.top()<ui.clip_rect().top()+100.0 { self.active_section=section_index; }
                        if self.jump.as_deref()==Some(&format!("section-{section_index}")) { heading.scroll_to_me(Some(Align::Min)); self.jump=None; self.active_section=section_index; }
                        ui.add_space(16.0);
                        for block in &section.blocks {
                            if block.runs.is_empty() { continue; }
                            let mark=self.marks.get(&block.id);
                            let highlighted=mark.is_some_and(|m| m.highlighted);
                            let bookmarked=mark.is_some_and(|m| m.bookmarked);
                            let noted=mark.is_some_and(|m| !m.note.is_empty());
                            let selected=self.selected.as_deref()==Some(block.id.as_str());
                            let mut job=egui::text::LayoutJob::default();
                            job.wrap.max_width=width-14.0;
                            for run in &block.runs {
                                if matches!(run.kind, RunKind::Link(_)) { continue; }
                                let rubric=matches!(run.kind,RunKind::Rubric);
                                let small=matches!(run.kind,RunKind::Small);
                                let size=if small { 10.5 } else if rubric && run.text.len()>90 { self.preferences.font_size*0.76 } else { self.preferences.font_size };
                                job.append(&run.text,0.0,egui::TextFormat {
                                    font_id:if small { FontId::proportional(size) } else { theme::reading(size) },
                                    color:if rubric { p.accent } else if small { p.muted } else { p.text },
                                    italics:matches!(run.kind,RunKind::Italic),
                                    line_height:Some(size*self.preferences.line_spacing),
                                    ..Default::default()
                                });
                            }
                            let frame=Frame::new().fill(if highlighted { p.highlight } else if selected { p.active.gamma_multiply(0.5) } else { Color32::TRANSPARENT }).corner_radius(3.0).inner_margin(Margin::symmetric(5,2));
                            let response=frame.show(ui,|ui| {
                                ui.set_max_width(width-10.0);
                                let label=ui.add(egui::Label::new(job).selectable(true).sense(Sense::click()));
                                for run in &block.runs {
                                    if let RunKind::Link(target)=&run.kind
                                        && let Some(anchor)=target.strip_prefix('#')
                                        && ui.link(RichText::new(run.text.trim()).size(12.0).color(p.accent)).clicked() {
                                            self.jump=Some(format!("anchor:{anchor}"));
                                    }
                                }
                                label
                            }).inner;
                            if self.jump.as_deref()==Some(block.id.as_str()) || block.anchors.iter().any(|a| self.jump.as_deref()==Some(&format!("anchor:{a}"))) {
                                response.scroll_to_me(Some(Align::Min)); self.jump=None; self.active_section=section_index;
                            }
                            if bookmarked || noted {
                                icons::draw(ui.painter(),egui::Rect::from_min_size(response.rect.right_top()+egui::vec2(8.0,8.0),Vec2::splat(12.0)),if bookmarked { Icon::Bookmark } else { Icon::Note },p.accent);
                            }
                            if response.double_clicked() { self.edit_note(block.id.clone()); }
                            response.context_menu(|ui| {
                                if ui.button(if bookmarked { "Remove bookmark" } else { "Bookmark passage" }).clicked() {
                                    if let Some(mut m)=self.mark_for(&block.id) { m.bookmarked = !m.bookmarked; self.save_mark(m); } ui.close();
                                }
                                if ui.button(if highlighted { "Remove highlight" } else { "Highlight passage" }).clicked() {
                                    if let Some(mut m)=self.mark_for(&block.id) { m.highlighted = !m.highlighted; self.save_mark(m); } ui.close();
                                }
                                if ui.button("Write a note…").clicked() { self.edit_note(block.id.clone()); ui.close(); }
                                if ui.button("Copy passage").clicked() { ctx.copy_text(block.text()); ui.close(); }
                            });
                            ui.add_space(14.0);
                        }
                    }
                    ui.add_space(30.0); ui.separator(); ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        let done=self.completed.contains(&self.hour);
                        if ui.add(egui::Button::new(RichText::new(if done { "Prayed · Mark as not prayed" } else { "Mark this hour as prayed" }).color(p.accent)).min_size(egui::vec2(220.0,40.0))).clicked() { self.toggle_complete(); }
                        ui.add_space(18.0);
                        ui.label(RichText::new("Texts provided by iBreviary. Original credits are retained above.").size(10.0).color(p.muted));
                        if ui.add_enabled(!self.preferences.offline, egui::Link::new(RichText::new("Visit iBreviary").size(10.0).color(p.accent))).on_hover_text("Open the source website. Requires a connection.").clicked() { ctx.open_url(egui::OpenUrl::new_tab("https://www.ibreviary.org/en/")); }
                    });
                    ui.add_space(65.0);
                    });
                });
            });
            self.scroll_offset=output.state.offset.y;
        });
    }

    fn calendar(&mut self, ctx: &egui::Context) {
        if !self.calendar_open {
            return;
        }
        let p = self.preferences.theme.palette();
        let mut open = true;
        egui::Window::new("Choose a day")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(310.0)
            .anchor(Align2::CENTER_TOP, [0.0, 90.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if icons::button(ui, Icon::Left, "Previous month", p.muted).clicked()
                        && let Some(month) = self.calendar_month.checked_sub_months(Months::new(1))
                        && crate::model::validate_date(month).is_ok()
                    {
                        self.calendar_month = month;
                    }
                    ui.label(
                        RichText::new(self.calendar_month.format("%B %Y").to_string())
                            .font(theme::heading(17.0)),
                    );
                    if icons::button(ui, Icon::Right, "Next month", p.muted).clicked()
                        && let Some(month) = self.calendar_month.checked_add_months(Months::new(1))
                        && crate::model::validate_date(month).is_ok()
                    {
                        self.calendar_month = month;
                    }
                });
                ui.add_space(12.0);
                let downloaded = self.library.downloaded_dates().unwrap_or_default();
                egui::Grid::new("calendar_grid")
                    .spacing(egui::vec2(5.0, 5.0))
                    .show(ui, |ui| {
                        for name in ["M", "T", "W", "T", "F", "S", "S"] {
                            ui.label(RichText::new(name).size(11.0).color(p.muted));
                        }
                        ui.end_row();
                        let leading = self.calendar_month.weekday().num_days_from_monday();
                        for cell in 0..42u32 {
                            let day = cell as i32 - leading as i32 + 1;
                            let date = if day > 0 {
                                self.calendar_month.with_day(day as u32)
                            } else {
                                None
                            };
                            if let Some(date) = date {
                                let count = downloaded
                                    .iter()
                                    .find(|(d, _)| *d == date)
                                    .map_or(0, |(_, c)| *c);
                                let button =
                                    egui::Button::new(
                                        RichText::new(format!(
                                            "{day}{}",
                                            if count > 0 { "·" } else { "" }
                                        ))
                                        .size(12.0)
                                        .color(if count > 0 { p.accent } else { p.text }),
                                    )
                                    .min_size(Vec2::splat(33.0))
                                    .selected(date == self.date);
                                if ui.add(button).clicked() {
                                    self.navigate(date, self.hour);
                                    self.calendar_open = false;
                                }
                            } else {
                                ui.allocate_space(Vec2::splat(33.0));
                            }
                            if cell % 7 == 6 {
                                ui.end_row();
                            }
                        }
                    });
                ui.add_space(8.0);
                ui.label(
                    RichText::new("A dot means prayers are downloaded.")
                        .small()
                        .color(p.muted),
                );
                ui.horizontal(|ui| {
                    let edit = ui.add(
                        TextEdit::singleline(&mut self.date_input)
                            .hint_text("YYYY-MM-DD")
                            .desired_width(140.0),
                    );
                    if ui.button("Go").clicked()
                        || (edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
                    {
                        match NaiveDate::parse_from_str(&self.date_input, "%Y-%m-%d") {
                            Ok(date) => {
                                self.navigate(date, self.hour);
                                self.calendar_open = false;
                            }
                            Err(_) => self.fail("Enter a date as YYYY-MM-DD."),
                        }
                    }
                    if ui.button("Today").clicked() {
                        self.navigate(Local::now().date_naive(), self.hour);
                        self.calendar_open = false;
                    }
                });
            });
        self.calendar_open &= open;
    }

    fn settings(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let p = self.preferences.theme.palette();
        let mut open = true;
        egui::Window::new("Reading appearance")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(355.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    for theme in Theme::ALL {
                        if ui
                            .add(
                                egui::Button::new(theme.title())
                                    .selected(self.preferences.theme == theme),
                            )
                            .clicked()
                        {
                            self.preferences.theme = theme;
                            theme.apply(ctx);
                        }
                    }
                });
                ui.add_space(18.0);
                ui.label(RichText::new("Text size").size(12.0).color(p.muted));
                ui.add(
                    egui::Slider::new(&mut self.preferences.font_size, 15.0..=30.0).suffix(" pt"),
                );
                ui.add_space(10.0);
                ui.label(RichText::new("Line spacing").size(12.0).color(p.muted));
                ui.add(
                    egui::Slider::new(&mut self.preferences.line_spacing, 1.25..=2.0)
                        .fixed_decimals(2),
                );
                ui.add_space(10.0);
                ui.label(RichText::new("Reading width").size(12.0).color(p.muted));
                ui.add(egui::Slider::new(&mut self.preferences.width, 460.0..=850.0).suffix(" px"));
                ui.add_space(22.0);
                ui.separator();
                ui.add_space(12.0);
                ui.label(
                    RichText::new("Ora et labora")
                        .font(theme::heading(25.0))
                        .color(p.accent),
                );
                ui.label(
                    RichText::new("Cinzel headings · Literata prayer text")
                        .size(11.0)
                        .color(p.muted),
                );
                ui.label(
                    RichText::new("Fonts are included with the app.")
                        .size(11.0)
                        .color(p.faint),
                );
                ui.add_space(10.0);
            });
        self.settings_open &= open;
    }

    fn library_window(&mut self, ctx: &egui::Context) {
        if !self.library_open {
            return;
        }
        let p = self.preferences.theme.palette();
        let mut open = true;
        egui::Window::new("Offline library").open(&mut open).collapsible(false).default_width(490.0).resizable(false).anchor(Align2::CENTER_CENTER,[0.0,0.0]).show(ctx,|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("Take your prayers with you.").font(theme::heading(19.0)));
            ui.add_space(8.0);
            ui.label("Downloaded offices stay on this device. Reading, search, notes, and bookmarks need no connection.");
            ui.add_space(15.0);
            if ui.checkbox(&mut self.preferences.offline,"Offline mode · disable downloads").changed() && self.preferences.offline
                && let Some(d)=&self.download { d.cancelled.store(true,Ordering::Relaxed); }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label("Starting"); ui.label(RichText::new(self.date.format("%B %-d, %Y").to_string()).color(p.accent));
                egui::ComboBox::from_id_salt("days").selected_text(format!("{} days",self.download_days)).show_ui(ui,|ui| {
                    for days in [1,3,7,14,30] { ui.selectable_value(&mut self.download_days,days,format!("{days} {}",if days==1 { "day" } else { "days" })); }
                });
            });
            if let Some(d)=&self.download {
                ui.add_space(8.0); ui.label(&d.message);
                ui.add(egui::ProgressBar::new(d.saved as f32/d.total as f32).text(format!("{} of {} offices saved",d.saved,d.total)));
                if ui.button("Stop download").clicked() { d.cancelled.store(true,Ordering::Relaxed); }
            } else if ui.add_enabled(!self.preferences.offline,egui::Button::new("Download all hours").min_size(egui::vec2(180.0,35.0))).clicked() {
                let dates=(0..self.download_days).filter_map(|i| self.date.checked_add_days(Days::new(u64::from(i)))).collect();
                self.start_download(dates,None,ctx);
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Existing prayers are kept. Retry resumes missing hours. The source may limit which dates it provides.").size(11.0).color(p.muted));
            ui.add_space(18.0); ui.separator(); ui.add_space(10.0);
            ui.label(RichText::new("DOWNLOADED DAYS").size(10.0).extra_letter_spacing(1.0).color(p.accent));
            let dates=match self.library.downloaded_dates() { Ok(v)=>v,Err(e)=>{self.fail(e);Vec::new()} };
            if dates.is_empty() { ui.label(RichText::new("No prayers downloaded yet.").size(12.0).color(p.muted)); }
            ScrollArea::vertical().id_salt("downloaded_dates").max_height(140.0).show(ui,|ui| {
                for (date,count) in dates {
                    ui.horizontal(|ui| {
                        if ui.button(date.format("%a, %B %-d, %Y").to_string()).clicked() { self.navigate(date,self.hour); self.library_open=false; }
                        ui.label(RichText::new(format!("{count}/5 hours")).size(11.0).color(if count==5 { p.accent } else { p.muted }));
                    });
                }
            });
            ui.add_space(12.0); ui.separator();
            egui::CollapsingHeader::new("Back up or move your library").show(ui,|ui| {
                ui.label(RichText::new("A backup contains your downloaded prayers, notes, bookmarks, and prayer marks.").size(11.0).color(p.muted));
                ui.add(TextEdit::singleline(&mut self.transfer_path).hint_text("/path/to/my-prayers.hours.json").desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.transfer_path.trim().is_empty(),egui::Button::new("Export backup")).clicked() {
                        match self.library.export_to(std::path::Path::new(self.transfer_path.trim())) { Ok(())=>self.tell("Library exported"),Err(e)=>self.fail(e) }
                    }
                    if ui.add_enabled(!self.transfer_path.trim().is_empty(),egui::Button::new("Import backup")).clicked() {
                        let path = PathBuf::from(self.transfer_path.trim()); self.import_backup(&path);
                    }
                });
                ui.label(RichText::new("You can also drop a backup file onto the app.").size(11.0).color(p.faint));
            });
            ui.add_space(8.0);
        });
        self.library_open &= open;
    }

    fn note_window(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.note.as_mut() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let p = self.preferences.theme.palette();
        egui::Window::new("A note for this passage")
            .open(&mut open)
            .default_width(420.0)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(truncate(&draft.anchor.quote, 240))
                        .font(theme::reading(14.0))
                        .color(p.muted),
                );
                ui.add_space(15.0);
                if ui
                    .add(
                        TextEdit::multiline(&mut draft.text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(7)
                            .hint_text("Your reflection…"),
                    )
                    .changed()
                {
                    draft.dirty = true;
                }
                ui.add_space(10.0);
                save = ui.button("Save note").clicked();
            });
        if save || !open {
            self.save_note();
            if self.note.as_ref().is_some_and(|n| !n.dirty) {
                self.note = None;
                self.tell("Note saved on this device");
            }
        }
    }

    fn help(&mut self, ctx: &egui::Context) {
        if !self.help_open {
            return;
        }
        let mut open = true;
        let p = self.preferences.theme.palette();
        egui::Window::new("Begin with one hour").open(&mut open).collapsible(false).default_width(440.0).show(ctx,|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new("A daily rhythm of prayer").font(theme::heading(21.0)).color(p.accent));
            ui.add_space(12.0);
            ui.label("Morning Prayer welcomes the day. Evening Prayer gives thanks as it closes. Night Prayer is prayed before sleep. You can begin with one of these and add the others over time.");
            ui.add_space(12.0);
            ui.label("The Office of Readings can be prayed at any time. Daytime Prayer contains the options for midmorning, midday, and midafternoon. Follow the rubrics for the hour you choose.");
            ui.add_space(12.0);
            ui.label("Read the prayer text and follow the red instructions. The source includes alternatives and the Invitatory where appropriate. You do not need to pray every alternative.");
            ui.add_space(18.0); ui.separator(); ui.add_space(10.0);
            for (key,action) in [("⌘1–5","Choose an hour"),("⌘K","Choose a date"),("⌘F","Search this hour"),("⌘J","Notes and bookmarks"),("⌘Enter","Mark as prayed"),("⌘+ / ⌘−","Adjust reading size"),("⌘B","Toggle the sidebar"),("Escape","Close panels or leave focus mode")] {
                ui.horizontal(|ui| { ui.label(RichText::new(key).monospace().color(p.accent)); ui.label(RichText::new(action).size(12.0)); });
            }
            ui.add_space(12.0);
            ui.label(RichText::new("Right-click any passage for bookmarks, highlights, notes, and copying.").size(12.0).color(p.muted));
        });
        self.help_open &= open;
    }
}

impl HoursApp {
    fn show(&mut self, ctx: &egui::Context) {
        self.frames += 1;
        self.receive_downloads();
        self.shortcuts(ctx);
        for file in ctx.input(|i| i.raw.dropped_files.clone()) {
            if let Some(path) = file.path {
                self.import_backup(&path);
            }
        }
        self.toolbar(ctx);
        self.status_bar(ctx);
        self.sidebar(ctx);
        self.right_panel(ctx);
        self.reader(ctx);
        self.calendar(ctx);
        self.settings(ctx);
        self.library_window(ctx);
        self.note_window(ctx);
        self.help(ctx);
        if let Some(error) = self.error.clone() {
            let mut open = true;
            egui::Window::new("Something needs attention")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(400.0)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(error);
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Saved prayers and notes are still in your library.").small(),
                    );
                    if ui.button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
            if !open {
                self.error = None;
            }
        }
        let serialized = serde_json::to_string(&self.preferences).unwrap();
        if serialized != self.saved_preferences {
            match self.library.save_setting("preferences", &serialized) {
                Ok(()) => self.saved_preferences = serialized,
                Err(e) => self.fail(e),
            }
        }
        if self.screenshot.is_some() {
            if self.frames == 8 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            let captures = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Screenshot { image, .. } = e {
                            Some(image.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            });
            for capture in captures {
                if let Some(path) = self.screenshot.take() {
                    let pixels: Vec<u8> =
                        capture.pixels.iter().flat_map(|p| p.to_array()).collect();
                    match image::save_buffer(
                        &path,
                        &pixels,
                        capture.width() as u32,
                        capture.height() as u32,
                        image::ColorType::Rgba8,
                    ) {
                        Ok(()) => {
                            eprintln!("Screenshot saved to {}", path.display());
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(e) => self.fail(e),
                    }
                }
            }
            ctx.request_repaint();
        }
        if self.download.is_some() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
}

impl eframe::App for HoursApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show(ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_note();
            if self.note.as_ref().is_some_and(|n| n.dirty) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
    }

    fn on_exit(&mut self) {
        self.save_note();
        self.persist_position();
        if let Some(d) = &self.download {
            d.cancelled.store(true, Ordering::Relaxed);
        }
    }
}

fn truncate(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let mut result: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Run, Section};

    fn office(date: NaiveDate, hour: Hour) -> Office {
        Office {
            date,
            hour,
            celebration: "Synthetic test office".into(),
            source_url: "https://ibreviary.com/m2/breviario.php?lang=en".into(),
            sections: vec![Section {
                title: "A test reading".into(),
                blocks: vec![Block {
                    id: "test-passage".into(),
                    anchors: vec![],
                    runs: vec![Run {
                        text: "A synthetic passage for testing local search and saved reflections."
                            .into(),
                        kind: RunKind::Text,
                    }],
                }],
            }],
        }
    }

    fn frame(app: &mut HoursApp, ctx: &egui::Context, key: Option<Key>, width: f32) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 800.0),
            )),
            ..Default::default()
        };
        if let Some(key) = key {
            input.modifiers = egui::Modifiers::COMMAND;
            input.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            });
        }
        let output = ctx.run(input, |ctx| app.show(ctx));
        assert!(
            !output.shapes.is_empty(),
            "The reader should paint a native UI"
        );
    }

    #[test]
    fn compact_window_keeps_the_offline_library_button_visible_and_clickable() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(&dir.path().join("hours.sqlite3")).unwrap();
        let ctx = egui::Context::default();
        let mut app = HoursApp::with_context(&ctx, library, Startup::default());
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(760.0, 560.0));
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let _ = ctx.run(input.clone(), |ctx| app.show(ctx));
        let output = ctx.run(input.clone(), |ctx| app.show(ctx));
        let button = output
            .shapes
            .iter()
            .find_map(|clipped| {
                if let egui::Shape::Text(text) = &clipped.shape
                    && text.galley.job.text == "Offline library"
                {
                    let rect = text.visual_bounding_rect();
                    assert!(screen.contains_rect(rect));
                    assert!(clipped.clip_rect.contains_rect(rect));
                    Some(rect.center())
                } else {
                    None
                }
            })
            .expect("Offline library must remain visible at minimum window size");
        for pressed in [true, false] {
            let mut click = input.clone();
            click.events = vec![
                egui::Event::PointerMoved(button),
                egui::Event::PointerButton {
                    pos: button,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            let _ = ctx.run(click, |ctx| app.show(ctx));
        }
        assert!(app.library_open);
    }

    #[test]
    fn backup_import_and_file_drop_preserve_the_current_reading_position() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(&dir.path().join("hours.sqlite3")).unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let mut document = office(date, Hour::Morning);
        let block = document.sections[0].blocks[0].clone();
        for i in 0..40 {
            let mut block = block.clone();
            block.id = format!("passage-{i}");
            document.sections[0].blocks.push(block);
        }
        library.save_office(&document).unwrap();
        let backup = dir.path().join("backup.json");
        library.export_to(&backup).unwrap();
        let ctx = egui::Context::default();
        let mut app = HoursApp::with_context(
            &ctx,
            library,
            Startup {
                date: Some(date),
                hour: Some(Hour::Morning),
                offline: true,
                ..Default::default()
            },
        );
        frame(&mut app, &ctx, None, 1220.0);
        app.restore_offset = Some(420.0);
        frame(&mut app, &ctx, None, 1220.0);
        assert_eq!(app.scroll_offset, 420.0);
        app.import_backup(&backup);
        frame(&mut app, &ctx, None, 1220.0);
        assert_eq!(app.scroll_offset, 420.0);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1220.0, 800.0),
            )),
            dropped_files: vec![egui::DroppedFile {
                path: Some(backup),
                ..Default::default()
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.show(ctx));
        assert_eq!(app.scroll_offset, 420.0);

        let next_date = date.succ_opt().unwrap();
        let incoming = Library::open(&dir.path().join("incoming.sqlite3")).unwrap();
        document.date = next_date;
        incoming.save_office(&document).unwrap();
        let next_backup = dir.path().join("next-day.json");
        incoming.export_to(&next_backup).unwrap();
        app.navigate(next_date, Hour::Morning);
        frame(&mut app, &ctx, None, 1220.0);
        assert!(app.office.is_none());
        app.import_backup(&next_backup);
        frame(&mut app, &ctx, None, 1220.0);
        assert!(app.office.is_some());
        assert_eq!(
            app.scroll_offset, 0.0,
            "A newly imported hour opens at its own position"
        );
    }

    #[test]
    fn note_keeps_its_passage_and_latest_bookmark_when_selection_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hours.sqlite3");
        let date = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let library = Library::open(&path).unwrap();
        let mut document = office(date, Hour::Morning);
        let mut second = document.sections[0].blocks[0].clone();
        second.id = "another-passage".into();
        document.sections[0].blocks.push(second);
        library.save_office(&document).unwrap();
        let ctx = egui::Context::default();
        let mut app = HoursApp::with_context(
            &ctx,
            library,
            Startup {
                date: Some(date),
                hour: Some(Hour::Morning),
                offline: true,
                ..Default::default()
            },
        );
        app.edit_note("test-passage".into());
        app.note.as_mut().unwrap().text = "This belongs to the first passage.".into();
        app.note.as_mut().unwrap().dirty = true;
        app.selected = Some("another-passage".into());
        let mut mark = app.mark_for("test-passage").unwrap();
        mark.bookmarked = true;
        app.save_mark(mark);
        app.save_note();
        let saved = app
            .library
            .annotation(date, Hour::Morning, "test-passage")
            .unwrap()
            .unwrap();
        assert_eq!(saved.note, "This belongs to the first passage.");
        assert!(saved.bookmarked);
        assert!(
            app.library
                .annotation(date, Hour::Morning, "another-passage")
                .unwrap()
                .is_none()
        );

        let mut mark = saved;
        mark.note.clear();
        app.save_mark(mark);
        app.edit_note("test-passage".into());
        app.note.as_mut().unwrap().text = "Keep the bookmark removed.".into();
        app.note.as_mut().unwrap().dirty = true;
        let mut mark = app.mark_for("test-passage").unwrap();
        mark.bookmarked = false;
        app.save_mark(mark);
        app.save_note();
        drop(app);
        let reopened = Library::open(&path).unwrap();
        let saved = reopened
            .annotation(date, Hour::Morning, "test-passage")
            .unwrap()
            .unwrap();
        assert_eq!(saved.note, "Keep the bookmark removed.");
        assert!(!saved.bookmarked);
    }

    #[test]
    fn offline_keyboard_navigation_search_notes_and_progress_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hours.sqlite3");
        let date = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let library = Library::open(&path).unwrap();
        library.save_office(&office(date, Hour::Morning)).unwrap();
        library.save_office(&office(date, Hour::Evening)).unwrap();
        let ctx = egui::Context::default();
        let mut app = HoursApp::with_context(
            &ctx,
            library,
            Startup {
                date: Some(date),
                hour: Some(Hour::Morning),
                offline: true,
                ..Default::default()
            },
        );
        frame(&mut app, &ctx, None, 1220.0);
        assert!(app.preferences.offline);
        assert!(app.download.is_none());
        frame(&mut app, &ctx, Some(Key::Num4), 1220.0);
        assert_eq!(app.office.as_ref().unwrap().hour, Hour::Evening);
        frame(&mut app, &ctx, Some(Key::F), 1220.0);
        assert!(app.panel == Panel::Search);
        app.query = "saved reflections".into();
        frame(&mut app, &ctx, None, 760.0);
        assert_eq!(app.office.as_ref().unwrap().search(&app.query).len(), 1);
        app.edit_note("test-passage".into());
        app.note.as_mut().unwrap().text = "Remember this after restarting.".into();
        app.note.as_mut().unwrap().dirty = true;
        app.save_note();
        let mut mark = app.mark_for("test-passage").unwrap();
        mark.bookmarked = true;
        app.save_mark(mark);
        app.toggle_complete();
        app.navigate(date, Hour::Morning);
        frame(&mut app, &ctx, None, 1220.0);
        assert!(app.download.is_none());
        drop(app);

        let library = Library::open(&path).unwrap();
        let mark = library
            .annotation(date, Hour::Evening, "test-passage")
            .unwrap()
            .unwrap();
        assert_eq!(mark.note, "Remember this after restarting.");
        assert!(mark.bookmarked);
        assert_eq!(library.completed(date).unwrap(), vec![Hour::Evening]);
        let mut reopened = HoursApp::with_context(
            &ctx,
            library,
            Startup {
                date: Some(date),
                hour: Some(Hour::Evening),
                offline: true,
                ..Default::default()
            },
        );
        frame(&mut reopened, &ctx, None, 1220.0);
        assert!(reopened.office.is_some());
        reopened.change_day(1);
        frame(&mut reopened, &ctx, None, 1220.0);
        assert!(
            reopened.office.is_none(),
            "A missing day must never show yesterday's office"
        );
        assert!(
            reopened.download.is_none(),
            "Navigation must never start a download"
        );
    }
}
