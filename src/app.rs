use crate::{
    engine::{self, Engines, FileInfo, Outcome, Progress, Request},
    settings::{DropArea, Settings},
};
use eframe::egui::{
    self, Align, Align2, Color32, FontFamily, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Vec2,
};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const TEXT: Color32 = Color32::from_gray(242);
const MUTED: Color32 = Color32::from_gray(190);
const WHITE: Color32 = Color32::from_gray(240);

enum Message {
    Inspected(u64, Result<FileInfo, String>),
    Progress(Progress),
    Finished(Result<Outcome, String>),
    Picked(Option<PathBuf>),
    Folder(Option<PathBuf>),
    Fullscreen(bool),
    Drag {
        armed: bool,
        proximity: f32,
        area: usize,
    },
    Settings,
    Browse,
    Place,
    Quit,
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum View {
    Idle,
    File,
    Progress,
    Done,
    Settings,
    Place,
    Error,
}

#[derive(Default)]
struct Hotspot {
    rects: Vec<[f32; 4]>,
    enabled: bool,
    scale: f32,
}

#[derive(Default)]
struct FormatWheel {
    position: f32,
    goal: f32,
    index: usize,
    selected: String,
    drag_start: f32,
    scroll: f32,
    qualities: HashMap<String, u8>,
}

#[cfg(debug_assertions)]
struct UiProbe {
    path: PathBuf,
    phase: u8,
    next: Instant,
    quality_rect: Option<Rect>,
    picker_rect: Option<Rect>,
    picker_press: Option<Pos2>,
    close_rect: Option<Rect>,
    quality_after: u8,
    disabled: bool,
    mode: String,
    observations: Vec<serde_json::Value>,
    completion_start: Option<Instant>,
    controls: HashMap<&'static str, Rect>,
}
#[cfg(debug_assertions)]
impl UiProbe {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            phase: 0,
            next: Instant::now(),
            quality_rect: None,
            picker_rect: None,
            picker_press: None,
            close_rect: None,
            quality_after: 100,
            disabled: std::env::var_os("DROPFORM_UI_EXPECT_DISABLED").is_some(),
            mode: std::env::var("DROPFORM_UI_PROBE_MODE").unwrap_or_default(),
            observations: Vec::new(),
            completion_start: None,
            controls: HashMap::new(),
        }
    }
    fn finish(&self, ctx: &egui::Context, report: serde_json::Value) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.path, serde_json::to_vec_pretty(&report).unwrap());
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

pub struct Dropflip {
    engines: Engines,
    settings: Settings,
    settings_return: Option<View>,
    active_area: usize,
    topmost: bool,
    fullscreen: bool,
    name_started: Instant,
    view: View,
    file: Option<Arc<FileInfo>>,
    format: String,
    format_wheel: FormatWheel,
    quality: u8,
    done_at: Option<Instant>,
    inspecting: bool,
    generation: u64,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    cancel: Arc<AtomicBool>,
    running: bool,
    quit_requested: bool,
    progress: Progress,
    outcome: Option<Outcome>,
    error: Option<String>,
    queue: VecDeque<PathBuf>,
    size: Vec2,
    last_frame: Instant,
    initial: Vec<PathBuf>,
    dragging: bool,
    hover_drop: bool,
    armed: bool,
    passthrough: bool,
    dialog: bool,
    glow: f32,
    closing: bool,
    popping: bool,
    proximity: f32,
    canvas: Vec2,
    last_view: View,
    motion_at: Instant,
    motion_from: Vec2,
    motion_target: Vec2,
    shell_texture: egui::TextureHandle,
    glow_texture: egui::TextureHandle,
    hotspot: Arc<Mutex<Hotspot>>,
    stop: Arc<AtomicBool>,
    _tray: Option<tray_icon::TrayIcon>,
    #[cfg(debug_assertions)]
    capture: Option<PathBuf>,
    #[cfg(debug_assertions)]
    capture_requested: bool,
    #[cfg(debug_assertions)]
    probe: Option<UiProbe>,
}

impl Drop for Dropflip {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Dropflip {
    pub fn new(cc: &eframe::CreationContext<'_>, engines: Engines, args: Vec<String>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/segoeui.ttf") {
            fonts
                .font_data
                .insert("Segoe".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, "Segoe".into());
        }
        let mut display = fonts.families[&FontFamily::Proportional].clone();
        if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/seguisb.ttf") {
            fonts.font_data.insert(
                "Segoe Semibold".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            display.insert(0, "Segoe Semibold".into());
        }
        fonts
            .families
            .insert(FontFamily::Name("Display".into()), display);
        cc.egui_ctx.set_fonts(fonts);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.panel_fill = Color32::TRANSPARENT;
        style.visuals.window_fill = Color32::from_gray(29);
        style.visuals.selection.bg_fill = Color32::from_gray(66);
        style.visuals.selection.stroke = Stroke::new(1.0_f32, TEXT);
        style.visuals.widgets.inactive.bg_fill = Color32::from_gray(43);
        style.visuals.widgets.inactive.weak_bg_fill = Color32::from_gray(43);
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        style.visuals.widgets.hovered.bg_fill = Color32::from_gray(58);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_gray(58);
        style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
        style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        style.visuals.widgets.active.bg_stroke = Stroke::NONE;
        style.visuals.widgets.open.bg_stroke = Stroke::NONE;
        style.visuals.widgets.active.bg_fill = Color32::from_gray(66);
        style.visuals.widgets.active.weak_bg_fill = Color32::from_gray(66);
        for visuals in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            visuals.corner_radius = egui::CornerRadius::same(10);
        }
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        style.spacing.interact_size = Vec2::new(36.0, 36.0);
        for (kind, size) in [
            (egui::TextStyle::Body, 13.0),
            (egui::TextStyle::Button, 13.0),
            (egui::TextStyle::Small, 11.5),
        ] {
            style.text_styles.insert(kind, FontId::proportional(size));
        }
        cc.egui_ctx.set_style(style);
        let (tx, rx) = mpsc::channel();
        let settings = Settings::load();
        let hotspot = Arc::new(Mutex::new(Hotspot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        watch_drag(
            hotspot.clone(),
            stop.clone(),
            tx.clone(),
            cc.egui_ctx.clone(),
        );
        let tray = make_tray(tx.clone(), cc.egui_ctx.clone());
        let error = tray
            .as_ref()
            .err()
            .map(|e| format!("Could not create tray menu: {e}"));
        Self {
            engines,
            settings,
            settings_return: None,
            active_area: 0,
            topmost: false,
            fullscreen: crate::platform::foreground_fullscreen(),
            name_started: Instant::now(),
            view: if error.is_some() || args.iter().any(|a| a == "--settings") {
                View::Settings
            } else {
                View::Idle
            },
            file: None,
            format: String::new(),
            format_wheel: FormatWheel::default(),
            quality: 100,
            done_at: None,
            inspecting: false,
            generation: 0,
            tx,
            rx,
            cancel: Arc::new(AtomicBool::new(false)),
            running: false,
            quit_requested: false,
            progress: Progress {
                fraction: None,
                message: String::new(),
            },
            outcome: None,
            error,
            queue: VecDeque::new(),
            size: Vec2::new(180.0, 110.0),
            last_frame: Instant::now(),
            initial: args
                .into_iter()
                .filter(|s| !s.starts_with("--"))
                .map(PathBuf::from)
                .collect(),
            dragging: false,
            hover_drop: false,
            armed: false,
            passthrough: true,
            dialog: false,
            glow: 0.0,
            closing: false,
            popping: false,
            proximity: 0.0,
            canvas: Vec2::new(180.0, 110.0),
            last_view: View::Idle,
            motion_at: Instant::now(),
            motion_from: Vec2::new(70.0, 28.0),
            motion_target: Vec2::ZERO,
            shell_texture: cc.egui_ctx.load_texture(
                "glass-shell",
                shell_pixels(),
                egui::TextureOptions::LINEAR,
            ),
            glow_texture: cc.egui_ctx.load_texture(
                "white-halo",
                halo_pixels(),
                egui::TextureOptions::LINEAR,
            ),
            hotspot,
            stop,
            _tray: tray.ok(),
            #[cfg(debug_assertions)]
            capture: std::env::var_os("DROPFORM_CAPTURE_PATH").map(PathBuf::from),
            #[cfg(debug_assertions)]
            capture_requested: false,
            #[cfg(debug_assertions)]
            probe: std::env::var_os("DROPFORM_UI_PROBE").map(|path| UiProbe::new(path.into())),
        }
    }
    fn save(&mut self) {
        self.settings.sync_legacy();
        if let Err(e) = self.settings.save() {
            self.error = Some(format!("Could not save settings: {e}"));
        }
    }
    fn ingest(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.running || self.inspecting {
            self.queue.push_back(path);
            return;
        }
        self.generation += 1;
        self.name_started = Instant::now() + Duration::from_millis(360);
        self.quality = 100;
        self.format_wheel = FormatWheel::default();
        let id = self.generation;
        self.inspecting = true;
        self.file = None;
        self.error = None;
        self.outcome = None;
        self.view = View::File;
        let e = self.engines.clone();
        let tx = self.tx.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            crate::platform::background_thread();
            let info = engine::inspect(&path, &e).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Message::Inspected(id, info));
            context.request_repaint();
        });
    }
    fn choose(&mut self, ctx: &egui::Context) {
        if self.dialog {
            return;
        }
        self.dialog = true;
        let tx = self.tx.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let p = rfd::FileDialog::new()
                .set_title("Convert a file")
                .pick_file();
            let _ = tx.send(Message::Picked(p));
            context.request_repaint();
        });
    }
    fn folder(&mut self, ctx: &egui::Context) {
        if self.dialog {
            return;
        }
        self.dialog = true;
        let tx = self.tx.clone();
        let context = ctx.clone();
        let path = self.settings.destination.clone();
        std::thread::spawn(move || {
            let p = rfd::FileDialog::new()
                .set_directory(path)
                .set_title("Save converted files here")
                .pick_folder();
            let _ = tx.send(Message::Folder(p));
            context.request_repaint();
        });
    }
    fn start(&mut self, ctx: &egui::Context) {
        if self.running {
            return;
        }
        let Some(info) = self.file.clone() else {
            return;
        };
        self.cancel = Arc::new(AtomicBool::new(false));
        self.running = true;
        self.view = View::Progress;
        self.progress = Progress {
            fraction: None,
            message: "Converting".into(),
        };
        self.error = None;
        let (compress, compression) = quality_compression(
            self.quality,
            info.formats
                .iter()
                .any(|f| f.ext == self.format && f.compression),
        );
        let request = Request {
            info: (*info).clone(),
            format: self.format.clone(),
            compress,
            compression,
            destination: self.settings.destination.clone(),
        };
        let e = self.engines.clone();
        let cancel = self.cancel.clone();
        let tx = self.tx.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let mut last = Instant::now() - Duration::from_secs(1);
            crate::platform::background_thread();
            let result = engine::convert(&request, &e, &cancel, &mut |p| {
                if last.elapsed() > Duration::from_millis(100) {
                    let _ = tx.send(Message::Progress(p));
                    context.request_repaint();
                    last = Instant::now();
                }
            })
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Message::Finished(result));
            context.request_repaint();
        });
    }
    fn messages(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Message::Inspected(id, result) if id == self.generation => {
                    self.inspecting = false;
                    match result {
                        Ok(file) => {
                            self.format = file
                                .formats
                                .iter()
                                .find(|f| f.ext != file.extension && f.ext != "zip")
                                .or(file.formats.first())
                                .map(|f| f.ext.clone())
                                .unwrap_or_default();
                            self.file = Some(Arc::new(file));
                        }
                        Err(e) => {
                            self.error = Some(e);
                            self.view = View::Error;
                        }
                    }
                }
                Message::Progress(p) => self.progress = p,
                Message::Finished(result) => {
                    self.running = false;
                    if self.quit_requested {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    match result {
                        Ok(out) => {
                            self.outcome = Some(out);
                            self.view = View::Done;
                            self.done_at = Some(Instant::now());
                            ctx.request_repaint_after(Duration::from_millis(2500));
                        }
                        Err(e) => {
                            if self.cancel.load(Ordering::Relaxed) {
                                self.view = View::File;
                                self.error = Some("Canceled".into());
                            } else {
                                self.error = Some(e);
                                self.view = View::Error;
                            }
                        }
                    }
                }
                Message::Picked(p) => {
                    self.dialog = false;
                    if let Some(p) = p {
                        self.ingest(p, ctx);
                    }
                }
                Message::Folder(p) => {
                    self.dialog = false;
                    if let Some(p) = p {
                        self.settings.destination = p;
                        self.save();
                    }
                }
                Message::Fullscreen(fullscreen) => self.fullscreen = fullscreen,
                Message::Drag {
                    armed,
                    proximity,
                    area,
                } => {
                    self.armed = armed;
                    self.proximity = proximity;
                    if self.view == View::Idle {
                        self.active_area = area.min(self.settings.areas.len() - 1);
                    }
                }
                Message::Settings => {
                    self.open_settings(false);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Message::Browse => self.choose(ctx),
                Message::Place if !self.running => self.view = View::Place,
                Message::Quit => {
                    self.cancel.store(true, Ordering::Relaxed);
                    self.quit_requested = true;
                    if !self.running {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                _ => {}
            }
        }
    }
    fn dismiss(&mut self) {
        if self.view == View::Settings {
            self.leave_settings();
            return;
        }
        self.done_at = None;
        self.view = if self.running {
            View::Progress
        } else {
            View::Idle
        };
        self.error = None;
        if !self.running {
            self.generation += 1;
            self.inspecting = false;
            self.file = None;
            self.outcome = None;
            self.queue.clear();
        }
    }
    fn next(&mut self, ctx: &egui::Context) {
        self.done_at = None;
        self.file = None;
        self.outcome = None;
        self.error = None;
        self.view = View::Idle;
        if let Some(p) = self.queue.pop_front() {
            self.ingest(p, ctx);
        }
    }
    fn open_settings(&mut self, from_panel: bool) {
        self.settings_return = if from_panel { Some(self.view) } else { None };
        self.view = View::Settings;
    }
    fn leave_settings(&mut self) {
        if let Some(view) = self.settings_return.take() {
            self.view = if self.running { View::Progress } else { view };
            self.name_started = Instant::now() + Duration::from_millis(360);
        } else {
            self.view = View::Idle;
            self.dismiss();
        }
    }
    fn header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, title: &str) {
        let (header, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
        ui.painter().text(
            header.left_center(),
            Align2::LEFT_CENTER,
            title,
            FontId::new(14.0, FontFamily::Name("Display".into())),
            TEXT,
        );
        let rect =
            Rect::from_min_size(header.right_top() - Vec2::new(32.0, 0.0), Vec2::splat(32.0));
        #[cfg(debug_assertions)]
        if let Some(p) = &mut self.probe {
            p.close_rect = Some(rect);
        }
        let returning = self.view == View::Settings && self.settings_return.is_some();
        if navigation_button(ui, rect, returning).clicked() {
            self.dismiss();
            ctx.request_repaint();
        }
    }
    fn file_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.inspecting {
            self.header(ui, ctx, "Reading…");
            ui.add_space(35.0);
            ui.horizontal(|ui| {
                ui.spinner();
            });
            return;
        }
        let Some(file) = self.file.clone() else {
            if ui.button("Choose file…").clicked() {
                self.choose(ctx);
            }
            return;
        };
        let (header, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::hover());
        let metadata = Rect::from_min_max(
            header.min,
            Pos2::new(header.right() - 72.0, header.bottom()),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(metadata)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.set_width(metadata.width());
                marquee(
                    ui,
                    &file.name,
                    self.name_started,
                    self.settings.reduced_motion,
                )
                .on_hover_text(file.path.display().to_string());
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} · {}", engine::size(file.bytes), file.summary))
                            .size(11.0)
                            .color(MUTED),
                    )
                    .truncate(),
                )
                .on_hover_text(format!(
                    "{} · {}{}",
                    engine::size(file.bytes),
                    file.summary,
                    file.note
                        .as_ref()
                        .map(|n| format!("\n{n}"))
                        .unwrap_or_default()
                ));
            },
        );
        let options_rect = Rect::from_min_size(
            Pos2::new(header.right() - 68.0, header.top() + 6.0),
            Vec2::new(32.0, 32.0),
        );
        let options = ui
            .interact(options_rect, ui.id().with("file-options"), Sense::click())
            .on_hover_text("Options");
        #[cfg(debug_assertions)]
        if let Some(probe) = &mut self.probe {
            probe.controls.insert("options", options_rect);
        }
        let hover = ctx.animate_bool_with_time(
            options.id.with("light"),
            options.hovered() || options.has_focus(),
            ui.style().animation_time,
        );
        ui.painter().rect_filled(
            options_rect.expand(hover),
            10,
            Color32::from_white_alpha((12.0 * hover) as u8),
        );
        for offset in [-4.5, 0.0, 4.5] {
            ui.painter().circle_filled(
                options_rect.center() + Vec2::new(offset * (1.0 + hover * 0.08), 0.0),
                1.2,
                if hover > 0.5 { WHITE } else { MUTED },
            );
        }
        options
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Options"));
        egui::Popup::menu(&options)
            .align(egui::RectAlign::BOTTOM_END)
            .frame(
                egui::Frame::popup(ui.style())
                    .corner_radius(18)
                    .inner_margin(8)
                    .stroke(Stroke::NONE)
                    .fill(Color32::from_gray(27)),
            )
            .show(|ui| {
                ui.set_min_width(180.0);
                if text_action(ui, "Choose file…", Vec2::new(180.0, 34.0)).clicked() {
                    self.choose(ctx);
                    ui.close();
                }
                if text_action(ui, "Settings", Vec2::new(180.0, 34.0)).clicked() {
                    self.open_settings(true);
                    ui.close();
                }
            });
        let close_rect = Rect::from_min_size(
            Pos2::new(header.right() - 32.0, header.top() + 6.0),
            Vec2::splat(32.0),
        );
        #[cfg(debug_assertions)]
        if let Some(p) = &mut self.probe {
            p.close_rect = Some(close_rect);
        }
        if close_button(ui, close_rect).clicked() {
            self.dismiss();
            ctx.request_repaint();
            return;
        }
        ui.add_space(4.0);
        let (picker, quality_rect) = format_picker(
            ui,
            &file.formats,
            &mut self.format,
            &mut self.format_wheel,
            &mut self.quality,
            self.settings.reduced_motion,
        );
        #[cfg(debug_assertions)]
        if let Some(probe) = &mut self.probe {
            probe.picker_rect = Some(picker.rect);
            probe.quality_rect = quality_rect;
        }
        #[cfg(not(debug_assertions))]
        let _ = (picker, quality_rect);
        let footer = Rect::from_min_size(
            Pos2::new(ui.max_rect().left(), ui.max_rect().bottom() - 70.0),
            Vec2::new(ui.available_width(), 70.0),
        );
        let mut footer_ui = ui.new_child(egui::UiBuilder::new().max_rect(footer));
        footer_ui.spacing_mut().interact_size.y = 26.0;
        footer_ui.horizontal(|ui| {
            if text_action(ui, "Change folder", Vec2::new(ui.available_width(), 26.0))
                .on_hover_text(format!(
                    "Choose output folder\n{}",
                    self.settings.destination.display()
                ))
                .clicked()
            {
                self.folder(ctx);
            }
        });
        if let Some(error) = &self.error {
            ui.add(egui::Label::new(RichText::new(error).size(11.5)).truncate())
                .on_hover_text(error);
        }
        if !self.queue.is_empty() {
            ui.label(
                RichText::new(format!("{} queued", self.queue.len()))
                    .size(11.0)
                    .color(MUTED),
            );
        }
        let text = if self.format == "zip" {
            "Create ZIP"
        } else {
            "Convert"
        };
        if primary(&mut footer_ui, text) {
            self.start(ctx);
        }
    }
    fn progress_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let body = ui.max_rect();
        ui.spacing_mut().interact_size.y = 28.0;
        ui.painter().text(
            body.left_top() + Vec2::new(0.0, 14.0),
            Align2::LEFT_CENTER,
            "Converting",
            FontId::proportional(13.0),
            TEXT,
        );
        let cancel_rect = Rect::from_min_size(
            body.right_top() - Vec2::new(62.0, 0.0),
            Vec2::new(62.0, 28.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(cancel_rect), |ui| {
            if ui
                .add_enabled(
                    !self.cancel.load(Ordering::Relaxed),
                    egui::Button::new("Cancel").frame(false),
                )
                .clicked()
            {
                self.cancel.store(true, Ordering::Relaxed);
            }
        });
        if let Some(f) = &self.file {
            let name = Rect::from_min_size(
                body.left_top() + Vec2::new(0.0, 36.0),
                Vec2::new(body.width(), 18.0),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(name)
                    .layout(Layout::top_down(Align::Min)),
            );
            marquee(
                &mut child,
                &f.name,
                self.name_started,
                self.settings.reduced_motion,
            )
            .on_hover_text(&self.progress.message);
        }
        let rect = Rect::from_min_size(
            body.left_bottom() - Vec2::new(0.0, 4.0),
            Vec2::new(body.width(), 4.0),
        );
        ui.painter().rect_filled(rect, 2, Color32::from_gray(60));
        if let Some(v) = self.progress.fraction {
            ui.painter().rect_filled(
                Rect::from_min_size(rect.min, Vec2::new(rect.width() * v.clamp(0.01, 1.0), 4.0)),
                2,
                WHITE,
            );
        } else {
            let phase = if self.settings.reduced_motion {
                0.4
            } else {
                (ctx.input(|i| i.time) as f32 * 0.45).fract()
            };
            let w = rect.width() * 0.28;
            let r = Rect::from_min_size(
                Pos2::new(rect.left() + (rect.width() + w) * phase - w, rect.top()),
                Vec2::new(w, 4.0),
            )
            .intersect(rect);
            ui.painter().rect_filled(r, 2, WHITE);
        }
    }
    fn done(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(out) = self.outcome.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new("Saved").strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let rect = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover()).0;
                if close_button(ui, rect).clicked() {
                    self.dismiss();
                    ctx.request_repaint();
                }
            });
        });
        ui.add(
            egui::Label::new(out.path.file_name().unwrap_or_default().to_string_lossy()).truncate(),
        );
        let saving = if out.bytes < out.original_bytes {
            format!(
                " · {:.0}% smaller",
                100.0 * (1.0 - out.bytes as f64 / out.original_bytes.max(1) as f64)
            )
        } else {
            String::new()
        };
        ui.label(
            RichText::new(format!("{}{}", engine::size(out.bytes), saving))
                .size(11.5)
                .color(MUTED),
        );
        ui.horizontal(|ui| {
            if ui.button("Show file").clicked() {
                if let Err(e) = crate::platform::reveal(&out.path) {
                    self.error = Some(e.to_string());
                    self.view = View::Error;
                }
            }
            if !self.queue.is_empty() && ui.button("Next file").clicked() {
                self.next(ctx);
            }
        });
    }
    fn preferences(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.header(ui, ctx, "Settings");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("area-selector")
                .selected_text(format!("Area {}", self.active_area + 1))
                .width(ui.available_width() - 80.0)
                .show_ui(ui, |ui| {
                    for index in 0..self.settings.areas.len() {
                        ui.selectable_value(
                            &mut self.active_area,
                            index,
                            format!("Area {}", index + 1),
                        );
                    }
                });
            let add = action_button(ui, "+", Vec2::splat(32.0), false);
            #[cfg(debug_assertions)]
            if let Some(probe) = &mut self.probe {
                probe.controls.insert("add", add.rect);
            }
            if add.on_hover_text("Add drop area").clicked() {
                let corner = ["bottom-left", "bottom-right", "top-left", "top-right"]
                    [self.settings.areas.len() % 4];
                self.settings.areas.push(DropArea {
                    corner: corner.into(),
                    ..Default::default()
                });
                self.active_area = self.settings.areas.len() - 1;
                self.save();
            }
            let remove = ui
                .add_enabled_ui(self.settings.areas.len() > 1, |ui| {
                    action_button(ui, "−", Vec2::splat(32.0), false)
                })
                .inner;
            if remove.on_hover_text("Remove this area").clicked() {
                self.settings.areas.remove(self.active_area);
                self.active_area = self.active_area.min(self.settings.areas.len() - 1);
                self.save();
            }
        });
        let active = self.active_area;
        ui.horizontal(|ui| {
            let corner_width = (ui.available_width() - ui.spacing().item_spacing.x) * 0.5;
            egui::ComboBox::from_id_salt("corner")
                .selected_text(if self.settings.areas[active].position.is_some() {
                    "Custom"
                } else {
                    corner_name(&self.settings.areas[active].corner)
                })
                .width(corner_width)
                .show_ui(ui, |ui| {
                    for value in ["bottom-left", "bottom-right", "top-left", "top-right"] {
                        if ui
                            .selectable_value(
                                &mut self.settings.areas[active].corner,
                                value.into(),
                                corner_name(value),
                            )
                            .clicked()
                        {
                            self.settings.areas[active].position = None;
                            self.save();
                        }
                    }
                });
            let move_area = action_button(
                ui,
                "Move area…",
                Vec2::new(ui.available_width(), 36.0),
                false,
            );
            #[cfg(debug_assertions)]
            if let Some(probe) = &mut self.probe {
                probe.controls.insert("move", move_area.rect);
            }
            if move_area.clicked() {
                self.view = View::Place;
            }
        });
        let bounds = crate::platform::work_area();
        let scale = ctx.pixels_per_point();
        let width = range_control(
            ui,
            "Width",
            &mut self.settings.areas[active].size[0],
            100.0,
            ((bounds[2] - bounds[0]) / scale - 20.0).min(1600.0),
        );
        let height = range_control(
            ui,
            "Height",
            &mut self.settings.areas[active].size[1],
            80.0,
            ((bounds[3] - bounds[1]) / scale - 20.0).min(1000.0),
        );
        #[cfg(debug_assertions)]
        if let Some(probe) = &mut self.probe {
            if probe.mode == "settings" {
                probe.quality_rect = Some(width.rect);
            }
        }
        if [&width, &height]
            .iter()
            .any(|r| r.drag_stopped() || (r.changed() && !r.dragged()))
        {
            self.save();
        }
        if text_action(ui, "Change folder", Vec2::new(ui.available_width(), 30.0))
            .on_hover_text(self.settings.destination.display().to_string())
            .clicked()
        {
            self.folder(ctx);
        }
        if toggle_row(ui, "Reduce motion", &mut self.settings.reduced_motion).changed() {
            self.save();
        }
        let above = toggle_row(
            ui,
            "Show above fullscreen",
            &mut self.settings.above_focused,
        );
        #[cfg(debug_assertions)]
        if let Some(probe) = &mut self.probe {
            probe.controls.insert("above", above.rect);
        }
        if above
            .on_hover_text("Keep drop areas available above fullscreen apps. Off by default.")
            .changed()
        {
            self.save();
        }
        if let Some(e) = &self.error {
            ui.add(egui::Label::new(e).truncate()).on_hover_text(e);
        }
        let footer = Rect::from_min_size(
            ui.max_rect().left_bottom() - Vec2::new(0.0, 36.0),
            Vec2::new(ui.available_width(), 36.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(footer));
        let done = action_button(&mut child, "Done", footer.size(), true);
        #[cfg(debug_assertions)]
        if let Some(probe) = &mut self.probe {
            probe.controls.insert("done", done.rect);
        }
        if done.clicked() {
            self.leave_settings();
        }
    }
    fn place(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let rect = ui.max_rect();
        let r = ui.interact(rect, ui.id().with("move-area"), Sense::drag());
        ui.painter()
            .rect_filled(rect, 14, Color32::from_white_alpha(18));
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            14,
            Stroke::new(1.0_f32, Color32::from_white_alpha(150)),
            StrokeKind::Inside,
        );
        r.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Move drop area")
        });
        if r.drag_started() {
            self.dragging = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let button = Rect::from_center_size(rect.center(), Vec2::new(88.0, 34.0));
        #[cfg(debug_assertions)]
        if let Some(probe) = &mut self.probe {
            probe.controls.insert("set", button);
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(button));
        if action_button(&mut child, "Set here", button.size(), true).clicked() {
            if let Some(r) = ctx.input(|i| i.viewport().outer_rect) {
                self.settings.areas[self.active_area].position = Some([r.left(), r.top()]);
                self.save();
            }
            self.view = View::Settings;
        }
    }
    fn error_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.header(ui, ctx, "Couldn’t convert");
        let error = self
            .error
            .as_deref()
            .unwrap_or("This file is not supported.");
        let short = error.chars().take(210).collect::<String>();
        ui.label(short).on_hover_text(error);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if self.file.is_some() && ui.button("Change format").clicked() {
                self.error = None;
                self.view = View::File;
            }
            if ui.button("Choose file…").clicked() {
                self.choose(ctx);
            }
            if ui.button("Dismiss").clicked() {
                self.next(ctx);
            }
        });
    }
    fn target_size(&self) -> Vec2 {
        match self.view {
            View::Idle | View::Place => Vec2::new(
                self.settings.areas[self.active_area].size[0],
                self.settings.areas[self.active_area].size[1],
            ),
            View::File => Vec2::new(
                320.0,
                286.0
                    + if self.error.is_some() { 20.0 } else { 0.0 }
                    + if !self.queue.is_empty() { 20.0 } else { 0.0 },
            ),
            View::Progress => Vec2::new(280.0, 112.0),
            View::Done => Vec2::new(290.0, 174.0),
            View::Settings => {
                Vec2::new(320.0, 432.0 + if self.error.is_some() { 20.0 } else { 0.0 })
            }
            View::Error => Vec2::new(340.0, 260.0),
        }
    }
}

impl eframe::App for Dropflip {
    #[cfg(debug_assertions)]
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let Some(mut probe) = self.probe.take() else {
            return;
        };
        ctx.request_repaint_after(Duration::from_millis(50));
        if Instant::now() < probe.next
            || self.inspecting
            || (probe.phase == 0 && self.motion_at.elapsed() < Duration::from_millis(650))
        {
            self.probe = Some(probe);
            return;
        }
        if probe.mode == "drag-fullscreen" {
            let sample = serde_json::json!({"view":format!("{:?}",self.view),"fullscreen":self.fullscreen,"topmost":self.topmost,"armed":self.armed,"hover":self.hover_drop,"file":self.file.as_ref().map(|f|f.name.clone()),"hotspot_enabled":self.hotspot.lock().unwrap().enabled});
            if probe.observations.last() != Some(&sample) {
                probe.observations.push(sample);
                let _ = std::fs::write(
                    &probe.path,
                    serde_json::to_vec_pretty(&serde_json::json!({"samples":probe.observations}))
                        .unwrap(),
                );
            }
            probe.next = Instant::now() + Duration::from_millis(100);
            self.probe = Some(probe);
            return;
        }
        if probe.mode == "placement" {
            if probe.phase == 0 {
                self.view = View::Place;
                probe.phase = 1;
            }
            self.probe = Some(probe);
            return;
        }
        if probe.mode == "menu" {
            if let Some(rect) = probe.controls.get("options") {
                raw.focused = true;
                if probe.phase < 2 {
                    raw.events.push(egui::Event::PointerMoved(rect.center()));
                    raw.events.push(egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed: probe.phase == 0,
                        modifiers: Default::default(),
                    });
                }
                probe.phase = probe.phase.saturating_add(1);
            }
            probe.next = Instant::now() + Duration::from_millis(200);
            self.probe = Some(probe);
            return;
        }
        if probe.mode == "settings-flow" {
            raw.focused = true;
            let click = |raw: &mut egui::RawInput, p: Pos2, pressed: bool| {
                raw.events.push(egui::Event::PointerMoved(p));
                raw.events.push(egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
            };
            let name = match probe.phase {
                1 | 2 => Some("add"),
                3 | 4 => Some("move"),
                5 | 6 => Some("set"),
                7 | 8 => Some("above"),
                9 | 10 => Some("done"),
                _ => None,
            };
            if let Some(name) = name {
                let Some(rect) = probe.controls.get(name).copied() else {
                    self.probe = Some(probe);
                    return;
                };
                click(raw, rect.center(), probe.phase % 2 == 1);
            }
            match probe.phase {
                0 => {
                    self.open_settings(true);
                }
                3 => {
                    probe.observations.push(serde_json::json!({"two_areas":self.settings.areas.len()==2,"returns_to_file":self.settings_return==Some(View::File)}));
                }
                5 => {
                    probe.observations.push(serde_json::json!({"placement":self.view==View::Place,"actual_size":self.canvas==Vec2::new(180.0,110.0)}));
                }
                7 => {
                    probe.observations.push(serde_json::json!({"set_returns_to_settings":self.view==View::Settings,"position_saved":Settings::load().areas[1].position.is_some()}));
                }
                11 => {
                    probe.observations.push(serde_json::json!({"done_returns_to_file":self.view==View::File&&self.file.is_some(),"panels_above_windowed_apps":self.topmost&&!self.fullscreen,"above_saved":Settings::load().above_focused}));
                    self.open_settings(true);
                }
                12 | 13 => {
                    click(raw, probe.close_rect.unwrap().center(), probe.phase == 12);
                }
                14 => {
                    probe.observations.push(serde_json::json!({"down_returns_to_file":self.view==View::File&&self.file.is_some()}));
                    self.open_settings(false);
                }
                15 | 16 => {
                    click(raw, probe.close_rect.unwrap().center(), probe.phase == 15);
                }
                17 => {
                    probe.observations.push(serde_json::json!({"direct_settings_closes":self.view==View::Idle,"areas_topmost_when_enabled":self.topmost,"both_targets_registered":self.hotspot.lock().unwrap().rects.len()==2}));
                    let passed = probe
                        .observations
                        .iter()
                        .all(|v| v.as_object().unwrap().values().all(|v| v == true));
                    probe.finish(ctx,serde_json::json!({"passed":passed,"input":"own-window raw pointer events; settings entry selected programmatically","samples":probe.observations}));
                    return;
                }
                _ => {}
            }
            probe.phase += 1;
            probe.next = Instant::now() + Duration::from_millis(500);
            self.probe = Some(probe);
            ctx.request_repaint();
            return;
        }
        if probe.mode == "folder" {
            let target = PathBuf::from(std::env::var_os("DROPFORM_QA_DESTINATION").unwrap());
            match probe.phase {
                0 => {
                    probe.observations.push(serde_json::json!({"default_downloads":self.settings.destination==Settings::default().destination}));
                    let _ = self.tx.send(Message::Folder(Some(target)));
                    probe.phase = 1;
                }
                1 => {
                    probe.observations.push(serde_json::json!({"selection_saved":self.settings.destination==target && Settings::load().destination==target}));
                    self.start(ctx);
                    probe.phase = 2;
                }
                _ if self.view == View::Done => {
                    let out = self.outcome.as_ref().unwrap();
                    let correct = out.path.parent().unwrap().canonicalize().ok()
                        == target.canonicalize().ok()
                        && out.path.is_file();
                    let passed = correct
                        && probe.observations[0]["default_downloads"] == true
                        && probe.observations[1]["selection_saved"] == true;
                    probe.finish(ctx,serde_json::json!({"passed":passed,"input":"folder result message","samples":probe.observations,"output":out.path}));
                    return;
                }
                _ => {}
            }
            if self.view == View::Error {
                probe.finish(ctx, serde_json::json!({"passed":false,"error":self.error}));
                return;
            }
            probe.next = Instant::now() + Duration::from_millis(100);
            self.probe = Some(probe);
            ctx.request_repaint();
            return;
        }
        if probe.mode == "quality-memory" {
            raw.focused = true;
            if probe.phase <= 2 {
                let Some(rect) = probe.quality_rect else {
                    self.probe = Some(probe);
                    return;
                };
                let point = Pos2::new(
                    if probe.phase == 0 {
                        rect.right() - 8.0
                    } else {
                        rect.left() + rect.width() * 0.35
                    },
                    rect.center().y,
                );
                raw.events.push(egui::Event::PointerMoved(point));
                if probe.phase != 1 {
                    raw.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: probe.phase == 0,
                        modifiers: Default::default(),
                    });
                }
            } else {
                probe.observations.push(serde_json::json!({"format":self.format,"quality":self.quality,"height":self.size.y,"slider":probe.quality_rect.is_some()}));
                match probe.phase {
                    3 => self.format = "webp".into(),
                    4 => self.format = "jpg".into(),
                    5 => self.format = "pdf".into(),
                    _ => {
                        let samples = &probe.observations;
                        let passed = samples[0]["quality"] == 36
                            && samples[1]["quality"] == 100
                            && samples[2]["quality"] == 36
                            && samples[3]["slider"] == false
                            && samples
                                .iter()
                                .all(|s| (s["height"].as_f64().unwrap() - 286.0).abs() < 0.1);
                        probe.finish(ctx, serde_json::json!({"passed":passed,"samples":samples}));
                        return;
                    }
                }
            }
            probe.phase += 1;
            probe.next = Instant::now() + Duration::from_millis(200);
            self.probe = Some(probe);
            ctx.request_repaint();
            return;
        }
        if probe.mode == "picker" {
            let Some(rect) = probe.picker_rect else {
                self.probe = Some(probe);
                return;
            };
            raw.focused = true;
            let center = rect.center();
            let pointer = |raw: &mut egui::RawInput, position: Pos2, down: Option<bool>| {
                raw.events.push(egui::Event::PointerMoved(position));
                if let Some(pressed) = down {
                    raw.events.push(egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    });
                }
            };
            match probe.phase {
                0 => {
                    probe
                        .observations
                        .push(serde_json::json!({"initial":self.format}));
                    pointer(
                        raw,
                        probe.quality_rect.map(|r| r.center()).unwrap_or(center),
                        None,
                    );
                    raw.events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, -40.0),
                        modifiers: Default::default(),
                    });
                }
                1 => {
                    probe
                        .observations
                        .push(serde_json::json!({"wheel":self.format}));
                    pointer(raw, center, Some(true));
                }
                2 => pointer(raw, center - Vec2::new(0.0, 34.0), None),
                3 => pointer(raw, center - Vec2::new(0.0, 34.0), Some(false)),
                4 => {
                    probe
                        .observations
                        .push(serde_json::json!({"drag":self.format}));
                    let point = center - Vec2::new(0.0, 30.0);
                    probe.picker_press = Some(point);
                    pointer(raw, point, Some(true));
                }
                5 => pointer(raw, probe.picker_press.unwrap(), Some(false)),
                6 | 7 => {
                    probe
                        .observations
                        .push(serde_json::json!({"format":self.format}));
                    raw.events.push(egui::Event::Key {
                        key: if probe.phase == 6 {
                            egui::Key::End
                        } else {
                            egui::Key::Home
                        },
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    });
                }
                _ => {
                    let samples = &probe.observations;
                    let passed = samples[0]["initial"] != samples[1]["wheel"]
                        && samples[1]["wheel"] != samples[2]["drag"]
                        && samples[2]["drag"] != samples[3]["format"]
                        && samples[4]["format"] == "zip"
                        && self.format == "png";
                    probe.finish(
                        ctx,
                        serde_json::json!({"passed":passed,"samples":samples,"home":self.format}),
                    );
                    return;
                }
            }
            probe.phase += 1;
            probe.next =
                Instant::now() + Duration::from_millis(if probe.phase == 4 { 650 } else { 200 });
            self.probe = Some(probe);
            return;
        }
        if probe.mode == "resize" {
            probe.observations.push(serde_json::json!({"phase":probe.phase,"height":self.size.y,"format":self.format,"slider":probe.quality_rect.is_some()}));
            match probe.phase {
                0 => self.format = "pdf".into(),
                1 | 3 => {}
                2 => self.format = "jpg".into(),
                _ => {
                    let samples = &probe.observations;
                    let height = |n: usize| samples[n]["height"].as_f64().unwrap();
                    let passed = (0..5).all(|n| (height(n) - 286.0).abs() < 0.1)
                        && samples[0]["slider"] == true
                        && samples[2]["slider"] == false
                        && samples[4]["slider"] == true;
                    probe.finish(ctx, serde_json::json!({"passed":passed,"samples":samples}));
                    return;
                }
            }
            probe.phase += 1;
            probe.next = Instant::now()
                + Duration::from_millis(if probe.phase == 2 || probe.phase == 4 {
                    400
                } else {
                    100
                });
            self.probe = Some(probe);
            ctx.request_repaint();
            return;
        }
        if probe.mode == "completion" {
            match probe.phase {
                0 => {
                    self.start(ctx);
                    probe.phase = 1;
                }
                1 if self.view == View::Done => {
                    probe.completion_start = self.done_at;
                    probe.phase = 2;
                }
                2 if probe
                    .completion_start
                    .is_some_and(|t| t.elapsed() >= Duration::from_millis(2200)) =>
                {
                    probe
                        .observations
                        .push(serde_json::json!({"visible_at_2200ms":self.view==View::Done}));
                    probe.phase = 3;
                }
                3 if self.view == View::Idle => {
                    probe.observations.push(serde_json::json!({"dismiss_ms":probe.completion_start.unwrap().elapsed().as_millis(),"fading":self.closing}));
                    probe.phase = 4;
                }
                4 if !self.closing => {
                    let ms = probe.observations[1]["dismiss_ms"].as_u64().unwrap();
                    let passed = probe.observations[0]["visible_at_2200ms"] == true
                        && (2500..2800).contains(&ms)
                        && probe.observations[1]["fading"] == true
                        && self.view == View::Idle
                        && self.outcome.is_none();
                    probe.finish(
                        ctx,
                        serde_json::json!({"passed":passed,"samples":probe.observations}),
                    );
                    return;
                }
                _ => {}
            }
            if self.view == View::Error {
                probe.finish(ctx, serde_json::json!({"passed":false,"error":self.error}));
                return;
            }
            probe.next = Instant::now() + Duration::from_millis(30);
            self.probe = Some(probe);
            return;
        }
        let Some(close) = probe.close_rect else {
            self.probe = Some(probe);
            return;
        };
        if probe.disabled && probe.phase == 0 {
            probe.observations.push(serde_json::json!({"quality_hidden":probe.quality_rect.is_none(),"height":self.size.y}));
            probe.phase = 3;
        }
        let quality = probe.quality_rect.unwrap_or(close);
        raw.focused = true;
        let point = match probe.phase {
            0 => Some((
                Pos2::new(quality.right() - 8.0, quality.center().y),
                Some(true),
            )),
            1 => Some((
                Pos2::new(quality.left() + quality.width() * 0.35, quality.center().y),
                None,
            )),
            2 => Some((
                Pos2::new(quality.left() + quality.width() * 0.35, quality.center().y),
                Some(false),
            )),
            3 => {
                probe.quality_after = if probe.mode == "settings" {
                    self.settings.area_size[0] as u8
                } else {
                    self.quality
                };
                Some((close.center(), Some(true)))
            }
            4 => Some((close.center(), Some(false))),
            _ => None,
        };
        if let Some((pos, pressed)) = point {
            raw.events.push(egui::Event::PointerMoved(pos));
            if let Some(pressed) = pressed {
                raw.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
            }
        } else {
            let changed = if probe.mode == "settings" {
                (probe.quality_after as f32 - 180.0).abs() > 1.0
            } else if probe.disabled {
                probe.quality_after == 100
            } else {
                probe.quality_after < 70
            };
            let closed = self.view == View::Idle;
            let hidden = !probe.disabled
                || probe.observations.first().is_some_and(|v| {
                    v["quality_hidden"] == true
                        && (v["height"].as_f64().unwrap() - 286.0).abs() < 0.1
                });
            let report = serde_json::json!({"quality_after":probe.quality_after,"hidden":probe.disabled,"quality_pass":changed&&hidden,"close_pass":closed,"view":format!("{:?}",self.view),"passed":changed&&hidden&&closed});
            if let Some(parent) = probe.path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&probe.path, serde_json::to_vec_pretty(&report).unwrap());
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        probe.phase += 1;
        probe.next = Instant::now() + Duration::from_millis(100);
        self.probe = Some(probe);
    }
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.messages(ctx);
        #[cfg(debug_assertions)]
        if let Some(path) = self.capture.clone() {
            for event in ctx.input(|i| i.raw.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let rgb = image
                        .pixels
                        .iter()
                        .flat_map(|c| {
                            let p = c.to_srgba_unmultiplied();
                            [0, 1, 2].map(|i| {
                                ((p[i] as u32 * p[3] as u32 + 28 * (255 - p[3] as u32)) / 255) as u8
                            })
                        })
                        .collect::<Vec<_>>();
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = image::save_buffer(
                        path,
                        &rgb,
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgb8,
                    );
                    self.capture = None;
                    if std::env::var_os("DROPFORM_CAPTURE_EXIT").is_some() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    break;
                }
            }
        }
        if self.running && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_requested = true;
            self.cancel.store(true, Ordering::Relaxed);
        }
        if !self.initial.is_empty() {
            for p in std::mem::take(&mut self.initial) {
                self.ingest(p, ctx);
            }
        }
        if self.view == View::Done {
            if let Some(start) = self.done_at {
                let remaining = Duration::from_millis(2500).saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    self.done_at = None;
                    self.view = View::Idle;
                } else {
                    ctx.request_repaint_after(remaining);
                }
            }
        }
        if self.view == View::Idle
            && self.closing
            && self.motion_at.elapsed() >= Duration::from_millis(300)
        {
            self.closing = false;
            if self.outcome.is_some() {
                self.next(ctx);
            }
        }
        let files = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        });
        for p in files {
            let (point, _) = crate::platform::pointer();
            let accept = self
                .hotspot
                .lock()
                .map(|h| {
                    h.rects.iter().any(|r| {
                        point[0] >= r[0] - 16.0
                            && point[0] <= r[2] + 16.0
                            && point[1] >= r[1] - 16.0
                            && point[1] <= r[3] + 16.0
                    })
                })
                .unwrap_or(false);
            if self.view != View::Idle
                || accept && (!self.fullscreen || self.settings.above_focused)
            {
                self.ingest(p, ctx);
            }
        }
        self.hover_drop = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.dismiss();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::O)) {
            self.choose(ctx);
        }
        let suppressed =
            self.fullscreen && !(self.view == View::Idle && self.settings.above_focused);
        let passthrough = suppressed || self.view == View::Idle && !self.armed && !self.hover_drop;
        if passthrough != self.passthrough {
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(passthrough));
            self.passthrough = passthrough;
        }
        let animation_time = if self.settings.reduced_motion {
            0.0
        } else {
            0.16
        };
        if ctx.style().animation_time != animation_time {
            ctx.style_mut(|style| style.animation_time = animation_time);
        }
        let topmost = !suppressed;
        if self.topmost != topmost {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if topmost {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
            self.topmost = topmost;
        }
        let dt = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.last_frame = Instant::now();
        let scale = ctx.pixels_per_point();
        let w = crate::platform::work_area();
        let work = Rect::from_min_max(
            Pos2::new(w[0] / scale, w[1] / scale),
            Pos2::new(w[2] / scale, w[3] / scale),
        );
        self.active_area = self.active_area.min(self.settings.areas.len() - 1);
        let regions = self
            .settings
            .areas
            .iter()
            .map(|area| area_rect(area, work))
            .collect::<Vec<_>>();
        let core = regions[self.active_area];
        let area = core.size();
        let area_pos = core.min;
        let selected = &self.settings.areas[self.active_area];
        let custom = selected.position.is_some();
        let right = selected.corner.ends_with("right");
        let bottom = selected.corner.starts_with("bottom");
        if let Ok(mut h) = self.hotspot.lock() {
            h.rects = regions
                .iter()
                .map(|r| {
                    [
                        r.left() * scale,
                        r.top() * scale,
                        r.right() * scale,
                        r.bottom() * scale,
                    ]
                })
                .collect();
            h.enabled = self.view == View::Idle && !suppressed;
            h.scale = scale;
        }
        if self.dragging && !crate::platform::pointer().1 {
            if let Some(r) = ctx.input(|i| i.viewport().outer_rect) {
                self.settings.areas[self.active_area].position = Some([r.left(), r.top()]);
                self.save();
            }
            self.dragging = false;
        }
        let halo = core.expand(230.0).intersect(work);
        let mut desired = self.target_size();
        if self.view == View::Place {
            desired = area;
        }
        desired.y = desired.y.min(work.height() - 20.0);
        if self.view != self.last_view || (self.view != View::File && desired != self.motion_target)
        {
            self.popping = self.last_view == View::Idle && self.view != View::Idle;
            self.closing = self.view == View::Idle && self.last_view != View::Idle;
            if self.view == View::File && self.last_view == View::Idle {
                self.glow = self.glow.max(0.75);
            }
            self.motion_from = if self.last_view == View::Idle && self.view != View::Idle {
                Vec2::new(70.0, 28.0)
            } else {
                self.size
            };
            self.motion_at = Instant::now();
            self.motion_target = desired;
            self.last_view = self.view;
        }
        self.motion_target = desired;
        let elapsed = self.motion_at.elapsed().as_secs_f32();
        let progress = if self.settings.reduced_motion {
            1.0
        } else {
            (elapsed / 0.36).clamp(0.0, 1.0)
        };
        let closing = self.closing && !self.settings.reduced_motion && elapsed < 0.30;
        let transition = (self.view != View::Idle && progress < 1.0) || closing;
        self.size = self.motion_from + (desired - self.motion_from) * pop_ease(progress);
        let next_canvas = if self.view == View::Idle {
            if closing {
                self.canvas.max(halo.size()).max(self.motion_from * 1.03)
            } else {
                halo.size()
            }
        } else if self.view == View::Place {
            area
        } else if transition {
            self.canvas
                .max(if self.popping {
                    desired * 1.06
                } else {
                    desired
                })
                .max(self.motion_from)
        } else {
            desired
        };
        if (next_canvas - self.canvas).length() > 0.5 {
            self.canvas = next_canvas;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.canvas));
        }
        if transition {
            ctx.request_repaint_after(Duration::from_millis(8));
        }
        let anchor_right = if custom {
            area_pos.x > work.center().x
        } else {
            right
        };
        let anchor_bottom = if custom {
            area_pos.y > work.center().y
        } else {
            bottom
        };
        let anchor = Pos2::new(
            if anchor_right {
                core.right()
            } else {
                core.left()
            },
            if anchor_bottom {
                core.bottom()
            } else {
                core.top()
            },
        );
        let position_for = |size: Vec2| {
            Pos2::new(
                if anchor_right {
                    anchor.x - size.x
                } else {
                    anchor.x
                },
                if anchor_bottom {
                    anchor.y - size.y
                } else {
                    anchor.y
                },
            )
        };
        let raw_pos = if self.view == View::Idle {
            halo.min
        } else {
            position_for(self.canvas)
        };
        let pos = Pos2::new(
            raw_pos
                .x
                .clamp(work.left(), (work.right() - self.canvas.x).max(work.left())),
            raw_pos
                .y
                .clamp(work.top(), (work.bottom() - self.canvas.y).max(work.top())),
        );
        if !self.dragging {
            if ctx.input(|i| {
                i.viewport()
                    .outer_rect
                    .map(|r| r.min.distance(pos) > 1.0)
                    .unwrap_or(true)
            }) {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            }
        }
        let target = if self.hover_drop && !suppressed {
            self.proximity.max(0.05)
        } else {
            0.0
        };
        if (self.glow - target).abs() > 0.01 {
            self.glow = if self.settings.reduced_motion {
                target
            } else {
                self.glow + (target - self.glow) * (1.0 - (-dt * 14.0).exp())
            };
            ctx.request_repaint_after(Duration::from_millis(8));
        } else {
            self.glow = target;
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                if suppressed {
                    return;
                }
                if self.view == View::Idle && self.glow <= 0.01 && !closing {
                    return;
                }
                if self.view == View::Idle {
                    if closing {
                        let t = (elapsed / 0.30).clamp(0.0, 1.0);
                        let retreat = if t < 0.12 {
                            -0.02 * (t / 0.12 * std::f32::consts::FRAC_PI_2).sin()
                        } else {
                            -0.02 + 1.02 * ease_out((t - 0.12) / 0.88)
                        };
                        let size =
                            self.motion_from + (Vec2::new(70.0, 28.0) - self.motion_from) * retreat;
                        let rect = Rect::from_min_size(position_for(size) - pos.to_vec2(), size)
                            .shrink(8.0);
                        glass(
                            ui.painter(),
                            &self.shell_texture,
                            rect,
                            Color32::from_white_alpha(((1.0 - smoothstep(t)) * 255.0) as u8),
                        );
                        if self.outcome.is_some() {
                            let content = Rect::from_min_size(
                                position_for(self.motion_from) - pos.to_vec2(),
                                self.motion_from,
                            )
                            .shrink(20.0);
                            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(content));
                            child.disable();
                            child.set_clip_rect(rect.shrink(10.0));
                            child.set_opacity(1.0 - smoothstep(t));
                            self.done(&mut child, ctx);
                        }
                    }
                    let corner = if !custom {
                        Pos2::new(
                            if anchor_right {
                                work.right()
                            } else {
                                work.left()
                            },
                            if anchor_bottom {
                                work.bottom()
                            } else {
                                work.top()
                            },
                        )
                    } else {
                        anchor
                    };
                    let spread = Vec2::new(220.0 + 200.0 * self.glow, 180.0 + 150.0 * self.glow);
                    let origin = Pos2::new(
                        if anchor_right {
                            corner.x - spread.x
                        } else {
                            corner.x
                        },
                        if anchor_bottom {
                            corner.y - spread.y
                        } else {
                            corner.y
                        },
                    ) - pos.to_vec2();
                    let uv = Rect::from_min_max(
                        Pos2::new(
                            if anchor_right { 1.0 } else { 0.0 },
                            if anchor_bottom { 1.0 } else { 0.0 },
                        ),
                        Pos2::new(
                            if anchor_right { 0.0 } else { 1.0 },
                            if anchor_bottom { 0.0 } else { 1.0 },
                        ),
                    );
                    ui.painter().image(
                        self.glow_texture.id(),
                        Rect::from_min_size(origin, spread),
                        uv,
                        Color32::from_white_alpha((self.glow.powf(0.85) * 200.0) as u8),
                    );
                    return;
                }
                if self.view == View::Place {
                    let mut child = ui.new_child(
                        egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::ZERO, area)),
                    );
                    self.place(&mut child, ctx);
                    return;
                }
                let rect = Rect::from_min_size(position_for(self.size) - pos.to_vec2(), self.size)
                    .shrink(8.0);
                let surface_alpha = if self.popping && !self.settings.reduced_motion {
                    smoothstep((elapsed / 0.16).clamp(0.0, 1.0))
                } else {
                    1.0
                };
                glass(
                    ui.painter(),
                    &self.shell_texture,
                    rect,
                    Color32::from_white_alpha((surface_alpha * 255.0) as u8),
                );
                let rim = self.glow * 0.5
                    + if transition {
                        0.45 * (1.0 - progress)
                    } else if self.running {
                        0.16
                    } else {
                        0.0
                    };
                if rim > 0.005 {
                    liquid_rim(
                        ui.painter(),
                        rect,
                        rim,
                        (ctx.input(|i| i.time) as f32 * 0.24).fract(),
                    );
                }
                let drift = if transition {
                    (position_for(self.size) - position_for(desired)) * 0.5
                } else {
                    Vec2::ZERO
                };
                let content =
                    Rect::from_min_size(position_for(desired) - pos.to_vec2() + drift, desired)
                        .shrink(20.0);
                ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
                    ui.set_clip_rect(rect.shrink(10.0));
                    if transition && !self.settings.reduced_motion {
                        ui.set_opacity(smoothstep(((elapsed - 0.04) / 0.24).clamp(0.0, 1.0)));
                    }
                    ui.set_min_width(content.width());
                    let scale = if transition && !self.settings.reduced_motion {
                        0.97 + 0.03 * pop_ease(progress)
                    } else {
                        1.0
                    };
                    let transform = egui::emath::TSTransform::from_translation(
                        content.center().to_vec2() * (1.0 - scale),
                    ) * egui::emath::TSTransform::from_scaling(scale);
                    ui.with_visual_transform(transform, |ui| match self.view {
                        View::File => self.file_view(ui, ctx),
                        View::Progress => self.progress_view(ui, ctx),
                        View::Done => self.done(ui, ctx),
                        View::Settings => self.preferences(ui, ctx),
                        View::Place => self.place(ui, ctx),
                        View::Error => self.error_view(ui, ctx),
                        View::Idle => {}
                    });
                });
            });
        #[cfg(debug_assertions)]
        if self.probe.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.running {
            ctx.request_repaint_after(Duration::from_millis(if self.settings.reduced_motion {
                500
            } else {
                33
            }));
        }
        #[cfg(debug_assertions)]
        if self.capture.is_some() && !self.capture_requested {
            ctx.request_repaint_after(Duration::from_millis(100));
            if self.motion_at.elapsed() > Duration::from_millis(650)
                && !self.inspecting
                && std::env::var("DROPFORM_CAPTURE_VIEW")
                    .ok()
                    .is_none_or(|wanted| {
                        if wanted == "menu" {
                            self.probe
                                .as_ref()
                                .is_some_and(|p| p.mode == "menu" && p.phase >= 4)
                        } else {
                            wanted == format!("{:?}", self.view).to_lowercase()
                        }
                    })
            {
                self.capture_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
        }
    }
}

fn format_picker(
    ui: &mut egui::Ui,
    formats: &[engine::Format],
    selected: &mut String,
    wheel: &mut FormatWheel,
    quality: &mut u8,
    reduced: bool,
) -> (egui::Response, Option<Rect>) {
    let (rect, mut response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 112.0),
        if formats.len() > 1 {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    if formats.is_empty() {
        return (response, None);
    }
    if wheel.selected != *selected {
        if !wheel.selected.is_empty() {
            wheel.qualities.insert(wheel.selected.clone(), *quality);
        }
        wheel.index = formats.iter().position(|f| f.ext == *selected).unwrap_or(0);
        wheel.position = wheel.index as f32;
        wheel.goal = wheel.position;
        wheel.selected = selected.clone();
        *quality = wheel.qualities.get(selected).copied().unwrap_or(100);
        wheel.scroll = 0.0;
    }
    let count = formats.len() as f32;
    if response.contains_pointer() && formats.len() > 1 {
        let delta = ui.input(|i| i.raw_scroll_delta.y);
        if delta.abs() > 0.0 {
            wheel.scroll -= delta / 40.0;
            let steps = wheel.scroll.round();
            if steps != 0.0 {
                wheel.goal += steps;
                wheel.scroll -= steps;
            }
            ui.input_mut(|i| {
                i.smooth_scroll_delta.y = 0.0;
                i.raw_scroll_delta.y = 0.0;
            });
        }
        ui.ctx().set_cursor_icon(if response.dragged() {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
    }
    if response.drag_started() {
        wheel.drag_start = wheel.position;
        response.request_focus();
    }
    if response.dragged() {
        wheel.position =
            wheel.drag_start - response.total_drag_delta().unwrap_or_default().y / 40.0;
        wheel.goal = wheel.position.round();
    } else if response.clicked() {
        if let Some(point) = response.interact_pointer_pos() {
            wheel.goal = (wheel.position + (point.y - rect.center().y) / 40.0).round();
            response.request_focus();
        }
    }
    if response.has_focus() {
        ui.input(|input| {
            if input.key_pressed(egui::Key::ArrowDown) || input.key_pressed(egui::Key::ArrowRight) {
                wheel.goal += 1.0;
            }
            if input.key_pressed(egui::Key::ArrowUp) || input.key_pressed(egui::Key::ArrowLeft) {
                wheel.goal -= 1.0;
            }
            if input.key_pressed(egui::Key::Home) {
                wheel.goal = (wheel.position / count).round() * count;
            }
            if input.key_pressed(egui::Key::End) {
                wheel.goal =
                    ((wheel.position - (count - 1.0)) / count).round() * count + count - 1.0;
            }
        });
    }
    if formats.len() == 1 {
        wheel.goal = 0.0;
        wheel.position = 0.0;
    }
    wheel.index = wheel.goal.round().rem_euclid(count) as usize;
    if formats[wheel.index].ext != *selected {
        wheel.qualities.insert(selected.clone(), *quality);
        *selected = formats[wheel.index].ext.clone();
        *quality = wheel.qualities.get(selected).copied().unwrap_or(100);
        wheel.selected = selected.clone();
        response.mark_changed();
        ui.ctx().request_repaint();
    }
    if !response.dragged() {
        let target = wheel.goal;
        if reduced || (wheel.position - target).abs() < 0.002 {
            wheel.position = target;
            let cycles = (wheel.position / count).floor() * count;
            wheel.position -= cycles;
            wheel.goal -= cycles;
        } else {
            let dt = ui.input(|i| i.unstable_dt).clamp(0.001, 0.032);
            wheel.position += (target - wheel.position) * (1.0 - (-dt * 22.0).exp());
            ui.ctx().request_repaint_after(Duration::from_millis(8));
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::slider(
            ui.is_enabled(),
            wheel.index as f64,
            format!("Output format: {}", formats[wheel.index].label),
        )
    });
    let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    p.rect_filled(
        Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 52.0)),
        14,
        Color32::from_gray(35),
    );
    let hover = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() || response.dragged() || response.has_focus(),
        if reduced { 0.0 } else { 0.15 },
    );
    let active_quality =
        if formats[wheel.index].compression && (wheel.goal - wheel.position).abs() < 0.3 {
            let y = rect.center().y + (wheel.goal - wheel.position) * 40.0;
            let area = Rect::from_min_max(
                Pos2::new(rect.left() + 16.0, y + 3.0),
                Pos2::new(rect.right() - 16.0, y + 25.0),
            );
            let control = quality_control(ui, area, quality);
            if control.changed() {
                wheel.qualities.insert(selected.clone(), *quality);
            }
            Some((
                area,
                control.has_focus(),
                control.hovered() || control.dragged(),
            ))
        } else {
            None
        };
    let first = if formats.len() == 1 {
        wheel.position.round() as i32
    } else {
        wheel.position.floor() as i32 - 2
    };
    let end = if formats.len() == 1 {
        first
    } else {
        wheel.position.ceil() as i32 + 2
    };
    for index in first..=end {
        let offset = (index as f32 - wheel.position) * 40.0;
        if offset.abs() > 68.0 {
            continue;
        }
        let fade = (1.0 - (offset.abs() / 64.0).powi(2)).clamp(0.0, 1.0)
            * (1.0 - (offset.abs() / 40.0).clamp(0.0, 1.0) * (0.12 * (1.0 - hover)));
        let mut painter = p.clone();
        painter.multiply_opacity(fade);
        let y = rect.center().y + offset;
        let format = &formats[index.rem_euclid(formats.len() as i32) as usize];
        let emphasis = (1.0 - offset.abs() / 40.0).clamp(0.0, 1.0);
        painter.text(
            Pos2::new(
                rect.left() + 16.0,
                y - if format.compression {
                    9.0 * emphasis
                } else {
                    0.0
                },
            ),
            Align2::LEFT_CENTER,
            &format.label,
            FontId::new(13.0 + 2.0 * emphasis, FontFamily::Name("Display".into())),
            TEXT,
        );
        if format.compression && emphasis > 0.01 {
            let value = if index.rem_euclid(formats.len() as i32) as usize == wheel.index {
                *quality
            } else {
                wheel.qualities.get(&format.ext).copied().unwrap_or(100)
            };
            let mut ink = painter.clone();
            ink.multiply_opacity(emphasis);
            ink.text(
                Pos2::new(rect.right() - 16.0, y - 9.0),
                Align2::RIGHT_CENTER,
                format!("{value}%"),
                FontId::proportional(11.0),
                MUTED,
            );
            let track = Rect::from_min_max(
                Pos2::new(rect.left() + 16.0, y + 13.0),
                Pos2::new(rect.right() - 16.0, y + 16.0),
            );
            let thumb = egui::lerp(track.left()..=track.right(), (value as f32 - 1.0) / 99.0);
            ink.rect_filled(track, 2, Color32::from_gray(75));
            ink.rect_filled(
                Rect::from_min_max(track.min, Pos2::new(thumb, track.bottom())),
                2,
                Color32::from_gray(180),
            );
            ink.circle_filled(Pos2::new(thumb, track.center().y), 3.5, WHITE);
        }
    }
    if let Some((area, focused, hovered)) = active_quality {
        if focused || hovered {
            let x = egui::lerp(area.left()..=area.right(), (*quality as f32 - 1.0) / 99.0);
            p.circle_filled(
                Pos2::new(x, area.bottom() - 7.5),
                7.0,
                Color32::from_white_alpha(25),
            );
        }
    }
    (
        response.on_hover_text(format!(
            "{}\nScroll, drag vertically, or click an option. Arrow keys also work.",
            formats[wheel.index].detail
        )),
        active_quality.map(|v| v.0),
    )
}

fn primary(ui: &mut egui::Ui, label: &str) -> bool {
    action_button(ui, label, Vec2::new(ui.available_width(), 36.0), true).clicked()
}
fn action_button(ui: &mut egui::Ui, label: &str, size: Vec2, primary: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(
        response.id.with("glow"),
        response.hovered() || response.has_focus(),
        ui.style().animation_time,
    );
    let scale = if response.is_pointer_button_down_on() {
        0.98
    } else {
        1.0 + hover * 0.015
    };
    let plate = Rect::from_center_size(rect.center(), rect.size() * scale);
    let p = ui.painter();
    for (spread, alpha) in [(6.0, 4.0), (3.0, 8.0)] {
        p.rect_filled(
            plate.expand(spread),
            18,
            Color32::from_white_alpha((alpha * hover) as u8),
        );
    }
    p.rect_filled(
        plate,
        14,
        if primary {
            WHITE
        } else {
            Color32::from_gray((35.0 + hover * 8.0) as u8)
        },
    );
    p.text(
        plate.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::new(13.0 * scale, FontFamily::Name("Display".into())),
        if primary {
            Color32::from_gray(20)
        } else {
            TEXT
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
fn text_action(ui: &mut egui::Ui, label: &str, size: Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let t = ui.ctx().animate_bool_with_time(
        response.id.with("light"),
        response.hovered() || response.has_focus(),
        ui.style().animation_time,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.0 + t * 0.2),
        Color32::from_gray((150.0 + 105.0 * t) as u8),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
fn toggle_row(ui: &mut egui::Ui, label: &str, value: &mut bool) -> egui::Response {
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    if response.clicked() || response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Space)) {
        *value = !*value;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(
        response.id.with("value"),
        *value,
        ui.style().animation_time,
    );
    let track = Rect::from_center_size(
        Pos2::new(rect.right() - 18.0, rect.center().y),
        Vec2::new(36.0, 20.0),
    );
    ui.painter().text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        TEXT,
    );
    ui.painter()
        .rect_filled(track, 10, Color32::from_gray((57.0 + 128.0 * t) as u8));
    ui.painter().circle_filled(
        Pos2::new(track.left() + 10.0 + 16.0 * t, track.center().y),
        7.0,
        if *value {
            Color32::from_gray(24)
        } else {
            WHITE
        },
    );
    if response.has_focus() {
        ui.painter()
            .circle_filled(track.center(), 14.0, Color32::from_white_alpha(20));
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
fn area_rect(area: &DropArea, work: Rect) -> Rect {
    let size = Vec2::new(
        area.size[0].min((work.width() - 20.0).max(1.0)),
        area.size[1].min((work.height() - 20.0).max(1.0)),
    );
    let default = Pos2::new(
        if area.corner.ends_with("right") {
            work.right() - size.x - 10.0
        } else {
            work.left() + 10.0
        },
        if area.corner.starts_with("bottom") {
            work.bottom() - size.y - 10.0
        } else {
            work.top() + 10.0
        },
    );
    let p = area
        .position
        .map(|p| Pos2::new(p[0], p[1]))
        .unwrap_or(default);
    Rect::from_min_size(
        Pos2::new(
            p.x.clamp(work.left(), work.right() - size.x),
            p.y.clamp(work.top(), work.bottom() - size.y),
        ),
        size,
    )
}
fn nearest_area(rects: &[[f32; 4]], point: [f32; 2]) -> (usize, f32) {
    rects
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let dx = (r[0] - point[0]).max(point[0] - r[2]).max(0.0);
            let dy = (r[1] - point[1]).max(point[1] - r[3]).max(0.0);
            (i, dx.hypot(dy))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, f32::INFINITY))
}
fn marquee_offset(distance: f32, elapsed: f32) -> f32 {
    if distance <= 0.0 {
        return 0.0;
    }
    let travel = distance / 32.0;
    let t = elapsed.max(0.0) % (4.0 + 2.0 * travel);
    if t < 2.0 {
        0.0
    } else if t < 2.0 + travel {
        distance * smoothstep((t - 2.0) / travel)
    } else if t < 4.0 + travel {
        distance
    } else {
        distance * (1.0 - smoothstep((t - 4.0 - travel) / travel))
    }
}
fn marquee(ui: &mut egui::Ui, name: &str, started: Instant, reduced: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    let galley = ui.painter().layout_no_wrap(
        name.into(),
        FontId::new(14.0, FontFamily::Name("Display".into())),
        TEXT,
    );
    let distance = (galley.size().x - rect.width()).max(0.0);
    let elapsed = Instant::now()
        .saturating_duration_since(started)
        .as_secs_f32();
    let offset = if reduced {
        0.0
    } else {
        marquee_offset(distance, elapsed)
    };
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(rect.min - Vec2::new(offset, 0.0), galley, TEXT);
    if distance > 0.0 && !reduced {
        let travel = distance / 32.0;
        let t = elapsed % (4.0 + 2.0 * travel);
        let delay = if t < 2.0 {
            2.0 - t
        } else if t >= 2.0 + travel && t < 4.0 + travel {
            4.0 + travel - t
        } else {
            0.016
        };
        ui.ctx()
            .request_repaint_after(Duration::from_secs_f32(delay.max(0.016)));
    }
    response.on_hover_text(name)
}
fn corner_name(value: &str) -> &str {
    match value {
        "bottom-right" => "Bottom right",
        "top-left" => "Top left",
        "top-right" => "Top right",
        _ => "Bottom left",
    }
}
fn pop_ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + 2.3 * t * t * t + 1.3 * t * t
}
fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn range_control(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    min: f32,
    max: f32,
) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 42.0),
        Sense::click_and_drag(),
    );
    let old = *value;
    let track = Rect::from_min_max(
        rect.left_bottom() + Vec2::new(5.0, -9.0),
        rect.right_bottom() - Vec2::new(5.0, 6.0),
    );
    if response.is_pointer_button_down_on() || response.clicked() {
        if let Some(point) = response.interact_pointer_pos() {
            *value = egui::lerp(
                min..=max,
                ((point.x - track.left()) / track.width()).clamp(0.0, 1.0),
            )
            .round();
            response.request_focus();
        }
    }
    if response.has_focus() {
        ui.input(|input| {
            let step = if input.modifiers.shift { 10.0 } else { 1.0 };
            if input.key_pressed(egui::Key::ArrowLeft) {
                *value -= step;
            }
            if input.key_pressed(egui::Key::ArrowRight) {
                *value += step;
            }
            if input.key_pressed(egui::Key::Home) {
                *value = min;
            }
            if input.key_pressed(egui::Key::End) {
                *value = max;
            }
        });
    }
    *value = value.clamp(min, max);
    if old != *value {
        response.mark_changed();
    }
    response.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), *value as f64, label));
    let painter = ui.painter();
    painter.text(
        rect.left_top() + Vec2::new(0.0, 10.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        MUTED,
    );
    painter.text(
        rect.right_top() + Vec2::new(0.0, 10.0),
        Align2::RIGHT_CENTER,
        format!("{value:.0} px"),
        FontId::proportional(12.0),
        TEXT,
    );
    let x = egui::lerp(track.left()..=track.right(), (*value - min) / (max - min));
    painter.rect_filled(track, 2, Color32::from_gray(58));
    painter.rect_filled(
        Rect::from_min_max(track.min, Pos2::new(x, track.bottom())),
        2,
        Color32::from_gray(170),
    );
    painter.circle_filled(
        Pos2::new(x, track.center().y),
        if response.dragged() { 5.5 } else { 4.5 },
        WHITE,
    );
    response
}

fn quality_compression(quality: u8, supported: bool) -> (bool, u8) {
    let quality = quality.clamp(1, 100);
    (supported && quality < 100, (100 - quality).clamp(1, 100))
}

fn close_button(ui: &mut egui::Ui, rect: Rect) -> egui::Response {
    navigation_button(ui, rect, false)
}
fn navigation_button(ui: &mut egui::Ui, rect: Rect, returning: bool) -> egui::Response {
    let response = ui.interact(rect, ui.id().with("close-panel"), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            if returning {
                "Back to conversion"
            } else {
                "Close"
            },
        )
    });
    let hover = ui.ctx().animate_bool_with_time(
        response.id.with("light"),
        response.hovered() || response.has_focus(),
        ui.style().animation_time,
    );
    ui.painter().rect_filled(
        rect.expand(hover),
        10,
        Color32::from_white_alpha((18.0 * hover) as u8),
    );
    let c = rect.center();
    let segments = if returning {
        [((-5.0, -2.5), (0.0, 2.5)), ((0.0, 2.5), (5.0, -2.5))]
    } else {
        [((-4.0, -4.0), (4.0, 4.0)), ((-4.0, 4.0), (4.0, -4.0))]
    };
    for (a, b) in segments {
        ui.painter().line_segment(
            [
                c + Vec2::new(a.0, a.1) * (1.0 + hover * 0.08),
                c + Vec2::new(b.0, b.1) * (1.0 + hover * 0.08),
            ],
            Stroke::new(1.5_f32, if response.hovered() { WHITE } else { MUTED }),
        );
    }
    response.on_hover_text(if returning {
        "Back to conversion · Escape"
    } else {
        "Close · Escape"
    })
}

fn quality_control(ui: &mut egui::Ui, rect: Rect, value: &mut u8) -> egui::Response {
    let available = ui.is_enabled();
    let mut response = ui.interact(
        rect,
        ui.id().with("format-quality"),
        if available {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let left = rect.left();
    let right = rect.right();
    let old = *value;
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if available && (response.clicked() || response.is_pointer_button_down_on()) {
        if let Some(p) = response.interact_pointer_pos() {
            *value = (1.0 + 99.0 * ((p.x - left) / (right - left)).clamp(0.0, 1.0)).round() as u8;
            response.request_focus();
        }
    }
    if available && response.has_focus() {
        ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowRight) {
                *value = value.saturating_add(1).min(100);
            }
            if i.key_pressed(egui::Key::ArrowLeft) {
                *value = value.saturating_sub(1).max(1);
            }
            if i.key_pressed(egui::Key::Home) {
                *value = 1;
            }
            if i.key_pressed(egui::Key::End) {
                *value = 100;
            }
        });
    }
    if *value != old {
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::slider(
            available,
            if available { *value as f64 } else { 100.0 },
            "Quality",
        )
    });
    response.on_hover_text(
        "Quality: drag horizontally. 100% uses the normal preset; lower values compress more.",
    )
}

fn halo_pixels() -> egui::ColorImage {
    let n = 256;
    let mut pixels = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let u = (x as f32 + 0.5) / n as f32;
            let v = (y as f32 + 0.5) / n as f32;
            let taper =
                |along: f32| (-(along / 0.58).powi(2)).exp() * smoothstep((1.0 - along) / 0.2);
            let band = |distance: f32| {
                0.9 * (-(distance / 0.004).powi(2)).exp()
                    + 0.46 * (-(distance / 0.045).powi(2)).exp()
                    + 0.18 * (-(distance / 0.18).powi(2)).exp()
            };
            let horizontal = band(v) * taper(u);
            let vertical = band(u) * taper(v);
            let a = ((1.0 - (1.0 - horizontal.min(1.0)) * (1.0 - vertical.min(1.0))).min(1.0)
                * 255.0) as u8;
            pixels.push(Color32::from_white_alpha(a));
        }
    }
    egui::ColorImage::new([n, n], pixels)
}

fn shell_pixels() -> egui::ColorImage {
    let n = 384;
    let mut pixels = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let px = (x as f32 + 0.5) * 0.5;
            let py = (y as f32 + 0.5) * 0.5;
            let qx = (px - 96.0).abs() - 52.0;
            let qy = (py - 96.0).abs() - 52.0;
            let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - 28.0;
            let t = ((py - 16.0) / 160.0).clamp(0.0, 1.0);
            let coverage = smoothstep((0.6 - d) / 1.2);
            let shadow = (-((d.max(0.0) / 4.8).powi(2))).exp() * 0.06;
            let rim =
                (-((d + 0.65) / 0.85).powi(2)).exp() * (0.35 + 0.65 * (1.0 - (px + py) / 384.0));
            let inner = (-((d + 2.4) / 2.0).powi(2)).exp();
            let body = coverage * (0.97 + 0.02 * (-(d / 1.0).powi(2)).exp());
            let shade = 27.0 - 5.0 * t
                + 3.0 * (-((t - 0.09) / 0.18).powi(2)).exp()
                + 68.0 * rim
                + 6.0 * inner;
            let alpha = body + shadow * (1.0 - body);
            let gray = if alpha > 0.0 {
                ((shade * body + 235.0 * shadow * (1.0 - body)) / alpha).clamp(0.0, 255.0) as u8
            } else {
                0
            };
            pixels.push(Color32::from_rgba_unmultiplied(
                gray,
                gray,
                gray,
                (alpha * 255.0) as u8,
            ));
        }
    }
    egui::ColorImage::new([n, n], pixels)
}

fn glass(p: &egui::Painter, texture: &egui::TextureHandle, rect: Rect, tint: Color32) {
    let outer = rect.expand(16.0);
    let border = 48.0_f32.min(outer.width() / 2.0).min(outer.height() / 2.0);
    let xs = [
        outer.left(),
        outer.left() + border,
        outer.right() - border,
        outer.right(),
    ];
    let ys = [
        outer.top(),
        outer.top() + border,
        outer.bottom() - border,
        outer.bottom(),
    ];
    let uv = [0.0, 48.0 / 192.0, 144.0 / 192.0, 1.0];
    for y in 0..3 {
        for x in 0..3 {
            p.image(
                texture.id(),
                Rect::from_min_max(Pos2::new(xs[x], ys[y]), Pos2::new(xs[x + 1], ys[y + 1])),
                Rect::from_min_max(Pos2::new(uv[x], uv[y]), Pos2::new(uv[x + 1], uv[y + 1])),
                tint,
            );
        }
    }
}

fn liquid_rim(p: &egui::Painter, rect: Rect, strength: f32, phase: f32) {
    let radius = 28.0_f32.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let w = (rect.width() - 2.0 * radius).max(0.0);
    let h = (rect.height() - 2.0 * radius).max(0.0);
    let arc = std::f32::consts::FRAC_PI_2 * radius;
    let lengths = [w, arc, h, arc, w, arc, h, arc];
    let total = lengths.iter().sum::<f32>();
    let point = |fraction: f32| {
        let mut d = fraction * total;
        for (index, len) in lengths.iter().enumerate() {
            if d <= *len || index == 7 {
                let t = (d / len.max(0.001)).clamp(0.0, 1.0);
                return match index {
                    0 => Pos2::new(rect.left() + radius + t * w, rect.top()),
                    2 => Pos2::new(rect.right(), rect.top() + radius + t * h),
                    4 => Pos2::new(rect.right() - radius - t * w, rect.bottom()),
                    6 => Pos2::new(rect.left(), rect.bottom() - radius - t * h),
                    _ => {
                        let (center, start) = match index {
                            1 => (
                                Pos2::new(rect.right() - radius, rect.top() + radius),
                                -std::f32::consts::FRAC_PI_2,
                            ),
                            3 => (
                                Pos2::new(rect.right() - radius, rect.bottom() - radius),
                                0.0,
                            ),
                            5 => (
                                Pos2::new(rect.left() + radius, rect.bottom() - radius),
                                std::f32::consts::FRAC_PI_2,
                            ),
                            _ => (
                                Pos2::new(rect.left() + radius, rect.top() + radius),
                                std::f32::consts::PI,
                            ),
                        };
                        let angle = start + t * std::f32::consts::FRAC_PI_2;
                        center + Vec2::new(angle.cos(), angle.sin()) * radius
                    }
                };
            }
            d -= *len;
        }
        rect.left_top()
    };
    for i in 0..96 {
        let t = i as f32 / 96.0;
        let dist = ((t - phase + 0.5).rem_euclid(1.0) - 0.5).abs();
        let light = (-((dist / 0.12).powi(2))).exp();
        let alpha = (strength * (0.12 + 0.88 * light) * 210.0).min(255.0) as u8;
        let points = [point(t), point((i + 1) as f32 / 96.0)];
        p.line_segment(
            points,
            Stroke::new(5.0_f32, Color32::from_white_alpha(alpha / 8)),
        );
        p.line_segment(
            points,
            Stroke::new(0.9_f32, Color32::from_white_alpha(alpha)),
        );
    }
}

fn watch_drag(
    hotspot: Arc<Mutex<Hotspot>>,
    stop: Arc<AtomicBool>,
    tx: Sender<Message>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let mut start = None;
        let mut previous = false;
        let mut previous_proximity = 0.0_f32;
        let mut previous_area = 0;
        let mut fullscreen = None;
        let mut fullscreen_check = Instant::now() - Duration::from_secs(1);
        let mut last_near = Instant::now() - Duration::from_secs(1);
        while !stop.load(Ordering::Relaxed) {
            if fullscreen_check.elapsed() >= Duration::from_millis(100) {
                let current = crate::platform::foreground_fullscreen();
                if fullscreen != Some(current) {
                    fullscreen = Some(current);
                    let _ = tx.send(Message::Fullscreen(current));
                    ctx.request_repaint();
                }
                fullscreen_check = Instant::now();
            }
            let (point, down) = crate::platform::pointer();
            if down {
                if start.is_none() {
                    start = Some((point, Instant::now()));
                }
            } else {
                start = None;
            }
            let dragging = start.is_some_and(|(origin, _)| {
                (point[0] - origin[0]).abs() + (point[1] - origin[1]).abs() > 3.0
            });
            let mut near = false;
            let mut proximity = 0.0;
            let mut area = previous_area;
            if let Ok(h) = hotspot.lock() {
                // Arm before entering the hotspot so Windows can discover the OLE target.
                near = h.enabled && dragging;
                if near {
                    last_near = Instant::now();
                }
                near = h.enabled && (near || last_near.elapsed() < Duration::from_millis(180));
                if near {
                    let (selected, distance) = nearest_area(&h.rects, point);
                    area = selected;
                    proximity =
                        smoothstep(1.0 - (distance / (230.0 * h.scale.max(1.0))).clamp(0.0, 1.0));
                }
            }
            if near != previous
                || area != previous_area
                || (proximity - previous_proximity).abs() > 0.008
            {
                let _ = tx.send(Message::Drag {
                    armed: near,
                    proximity,
                    area,
                });
                ctx.request_repaint();
                previous = near;
                previous_proximity = proximity;
                previous_area = area;
            }
            std::thread::sleep(Duration::from_millis(if down { 8 } else { 64 }));
        }
    });
}

fn make_tray(tx: Sender<Message>, ctx: egui::Context) -> anyhow::Result<tray_icon::TrayIcon> {
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem},
        TrayIconBuilder,
    };
    let menu = Menu::new();
    let browse = MenuItem::new("Choose file…", true, None);
    let settings = MenuItem::new("Settings", true, None);
    let place = MenuItem::new("Move drop area…", true, None);
    let quit = MenuItem::new("Quit Dropflip", true, None);
    menu.append_items(&[&browse, &settings, &place, &quit])?;
    let b = browse.id().clone();
    let s = settings.id().clone();
    let p = place.id().clone();
    let q = quit.id().clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let m = if event.id == b {
            Message::Browse
        } else if event.id == s {
            Message::Settings
        } else if event.id == p {
            Message::Place
        } else if event.id == q {
            Message::Quit
        } else {
            return;
        };
        let _ = tx.send(m);
        ctx.request_repaint();
    }));
    Ok(TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Dropflip · drag files to your drop area")
        .with_icon(crate::branding::tray_icon()?)
        .build()?)
}

#[cfg(test)]
mod visual_tests {
    use super::*;
    #[test]
    fn filename_waits_at_both_ends_before_reversing() {
        assert_eq!(marquee_offset(64.0, 1.99), 0.0);
        assert!((marquee_offset(64.0, 3.0) - 32.0).abs() < 0.01);
        assert_eq!(marquee_offset(64.0, 4.5), 64.0);
        assert_eq!(marquee_offset(64.0, 5.99), 64.0);
        assert!((marquee_offset(64.0, 7.0) - 32.0).abs() < 0.01);
        assert_eq!(marquee_offset(64.0, 8.5), 0.0);
    }
    #[test]
    fn areas_stay_on_screen_and_drag_targets_choose_the_nearest() {
        let work = Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1080.0));
        let a = area_rect(&DropArea::default(), work);
        let b = area_rect(
            &DropArea {
                corner: "top-right".into(),
                size: [800.0, 400.0],
                position: None,
            },
            work,
        );
        let r = [a, b].map(|r| [r.left(), r.top(), r.right(), r.bottom()]);
        assert_eq!(nearest_area(&r, [30.0, 1000.0]).0, 0);
        assert_eq!(nearest_area(&r, [1800.0, 30.0]), (1, 0.0));
        let huge = area_rect(
            &DropArea {
                size: [3000.0, 2000.0],
                position: Some([-900.0, 2000.0]),
                ..Default::default()
            },
            work,
        );
        assert!(work.contains_rect(huge));
        assert!(!Settings::default().above_focused);
    }
    #[test]
    fn quality_starts_uncompressed_and_lower_values_increase_compression() {
        assert_eq!(quality_compression(100, true), (false, 1));
        assert_eq!(quality_compression(70, true), (true, 30));
        assert_eq!(quality_compression(25, true), (true, 75));
        assert!(!quality_compression(25, false).0);
    }
    #[test]
    fn corner_glow_stays_on_edges_and_glass_keeps_its_translucency() {
        let glow = halo_pixels();
        let alpha = |x: usize, y: usize| glow.pixels[y * glow.size[0] + x].a();
        assert!(alpha(100, 1) > alpha(100, 100) * 8);
        assert!(alpha(1, 100) > alpha(100, 100) * 8);
        assert!(alpha(250, 250) < 2);
        assert!(
            alpha(100, 30) > 8 && alpha(100, 100) < 3,
            "The glow should reach inward while remaining anchored to the screen edges"
        );
        let shell = shell_pixels();
        assert!((245..=253).contains(&shell.pixels[192 * 384 + 192].a()));
        if let Some(directory) = std::env::var_os("DROPFORM_VISUAL_QA") {
            let directory = PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            for (name, pixels) in [("corner-glow.png", glow), ("glass-shell.png", shell)] {
                let bytes = pixels
                    .pixels
                    .iter()
                    .flat_map(|p| p.to_srgba_unmultiplied())
                    .collect::<Vec<_>>();
                image::save_buffer(
                    directory.join(name),
                    &bytes,
                    pixels.size[0] as u32,
                    pixels.size[1] as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                let opaque = bytes
                    .chunks_exact(4)
                    .flat_map(|p| {
                        [0, 1, 2].map(|i| {
                            ((p[i] as u32 * p[3] as u32 + 28 * (255 - p[3] as u32)) / 255) as u8
                        })
                    })
                    .collect::<Vec<_>>();
                image::save_buffer(
                    directory.join(name.replace(".png", "-on-gray.png")),
                    &opaque,
                    pixels.size[0] as u32,
                    pixels.size[1] as u32,
                    image::ColorType::Rgb8,
                )
                .unwrap();
            }
        }
    }
}
