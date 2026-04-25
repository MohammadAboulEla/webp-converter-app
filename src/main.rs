#![windows_subsystem = "windows"]

use eframe::NativeOptions;
use eframe::egui::{self, Color32, FontFamily, FontId, RichText, Style};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use webp_converter_app::{LogEvent, convert_to_webp_dir_threads};

const STORAGE_KEY: &str = "webp_converter_app_state";
const LOG_CAP: usize = 5000;
const LOG_TRIM_TO: usize = 4000;

fn main() -> Result<(), eframe::Error> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([620.0, 400.0])
            .with_position([240.0, 280.0])
            .with_min_inner_size([565.0, 280.0]),
        ..Default::default()
    };

    eframe::run_native(
        "WebP Converter App",
        options,
        Box::new(|cc| {
            let mut style: Style = (*cc.egui_ctx.global_style()).clone();
            style.override_font_id = Some(FontId::new(20.0, FontFamily::Proportional));
            cc.egui_ctx.set_global_style(style);
            Ok(Box::new(MyApp::new(cc)))
        }),
    )
}

struct MyApp {
    input_path: String,
    output_path: String,
    quality: f32,
    lossless: bool,
    log_errors_only: bool,

    log: Arc<Mutex<Vec<LogEvent>>>,
    is_running: Arc<AtomicBool>,
    total: Arc<AtomicUsize>,
    done: Arc<AtomicUsize>,
    errors: Arc<AtomicUsize>,

    validation_error: Option<String>,
    pending_input: Arc<Mutex<Option<String>>>,
    pending_output: Arc<Mutex<Option<String>>>,
}

impl MyApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            input_path: String::new(),
            output_path: String::new(),
            quality: 87.0,
            lossless: false,
            log_errors_only: false,
            log: Arc::new(Mutex::new(Vec::new())),
            is_running: Arc::new(AtomicBool::new(false)),
            total: Arc::new(AtomicUsize::new(0)),
            done: Arc::new(AtomicUsize::new(0)),
            errors: Arc::new(AtomicUsize::new(0)),
            validation_error: None,
            pending_input: Arc::new(Mutex::new(None)),
            pending_output: Arc::new(Mutex::new(None)),
        };
        if let Some(storage) = cc.storage {
            if let Some(raw) = storage.get_string(STORAGE_KEY) {
                app.load_from_storage(&raw);
            }
        }
        app
    }

    fn load_from_storage(&mut self, raw: &str) {
        for line in raw.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "input_path" => self.input_path = v.to_string(),
                "output_path" => self.output_path = v.to_string(),
                "quality" => {
                    if let Ok(q) = v.parse::<f32>() {
                        self.quality = q.clamp(0.0, 100.0);
                    }
                }
                "lossless" => self.lossless = v == "true",
                "log_errors_only" => self.log_errors_only = v == "true",
                _ => {}
            }
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.input_path.trim().is_empty() {
            return Err("Input directory is empty.".into());
        }
        if self.output_path.trim().is_empty() {
            return Err("Output directory is empty.".into());
        }
        let in_path = Path::new(&self.input_path);
        if !in_path.exists() {
            return Err("Input directory does not exist.".into());
        }
        if !in_path.is_dir() {
            return Err("Input path is not a directory.".into());
        }
        if Path::new(&self.input_path) == Path::new(&self.output_path) {
            return Err("Input and output directories must differ.".into());
        }
        Ok(())
    }

    fn convert_in_thread(&mut self, ctx: &egui::Context) {
        if let Err(msg) = self.validate() {
            self.validation_error = Some(msg);
            return;
        }
        self.validation_error = None;

        let input = self.input_path.clone();
        let output = self.output_path.clone();
        let quality = self.quality;
        let lossless = self.lossless;
        let log = self.log.clone();
        let is_running = self.is_running.clone();
        let total = self.total.clone();
        let done = self.done.clone();
        let errors = self.errors.clone();
        let ctx = ctx.clone();

        if let Ok(mut log) = self.log.lock() {
            log.clear();
        }
        total.store(0, Ordering::Relaxed);
        done.store(0, Ordering::Relaxed);
        errors.store(0, Ordering::Relaxed);
        is_running.store(true, Ordering::Relaxed);

        std::thread::spawn(move || {
            let log_fn = {
                let log = log.clone();
                let ctx = ctx.clone();
                let total = total.clone();
                let done = done.clone();
                let errors = errors.clone();
                move |event: LogEvent| {
                    match &event {
                        LogEvent::Discovered { total: t } => {
                            total.store(*t, Ordering::Relaxed);
                        }
                        LogEvent::Converted { .. } | LogEvent::Skipped { .. } => {
                            done.fetch_add(1, Ordering::Relaxed);
                        }
                        LogEvent::Error { .. } => {
                            done.fetch_add(1, Ordering::Relaxed);
                            errors.fetch_add(1, Ordering::Relaxed);
                        }
                        _ => {}
                    }
                    if let Ok(mut log) = log.lock() {
                        if log.len() >= LOG_CAP {
                            let drop = log.len() - LOG_TRIM_TO;
                            log.drain(0..drop);
                        }
                        log.push(event);
                    }
                    ctx.request_repaint();
                }
            };

            if let Err(e) =
                convert_to_webp_dir_threads(&input, &output, quality, lossless, log_fn)
            {
                if let Ok(mut log) = log.lock() {
                    log.push(LogEvent::Error {
                        msg: format!("Fatal: {}", e),
                    });
                }
            }
            is_running.store(false, Ordering::Relaxed);
            ctx.request_repaint();
        });
    }

    fn spawn_folder_picker(&self, slot: Arc<Mutex<Option<String>>>, ctx: &egui::Context) {
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                if let Ok(mut s) = slot.lock() {
                    *s = Some(path.display().to_string());
                }
                ctx.request_repaint();
            }
        });
    }

    fn drain_pending_pickers(&mut self) {
        if let Ok(mut s) = self.pending_input.lock() {
            if let Some(p) = s.take() {
                self.input_path = p;
            }
        }
        if let Ok(mut s) = self.pending_output.lock() {
            if let Some(p) = s.take() {
                self.output_path = p;
            }
        }
    }

    fn ui_paths(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let label_w = 180.0;
        let btn_w = 32.0;
        let row_h = 30.0;
        let spacing = ui.spacing().item_spacing.x;
        let edit_w =
            (ui.available_width() - label_w - btn_w - spacing * 2.0).max(60.0);

        let mut clicked_input = false;
        ui.horizontal(|ui| {
            ui.add_sized(
                [label_w, row_h],
                egui::Label::new("Input Directory:").truncate(),
            );
            ui.add_sized(
                [edit_w, row_h],
                egui::TextEdit::singleline(&mut self.input_path),
            );
            if ui
                .add_sized([btn_w, row_h], egui::Button::new("📁"))
                .clicked()
            {
                clicked_input = true;
            }
        });
        if clicked_input {
            self.spawn_folder_picker(self.pending_input.clone(), ctx);
        }

        let mut clicked_output = false;
        ui.horizontal(|ui| {
            ui.add_sized(
                [label_w, row_h],
                egui::Label::new("Output Directory:").truncate(),
            );
            ui.add_sized(
                [edit_w, row_h],
                egui::TextEdit::singleline(&mut self.output_path),
            );
            if ui
                .add_sized([btn_w, row_h], egui::Button::new("📁"))
                .clicked()
            {
                clicked_output = true;
            }
        });
        if clicked_output {
            self.spawn_folder_picker(self.pending_output.clone(), ctx);
        }
    }

    fn ui_convert_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let running = self.is_running.load(Ordering::Relaxed);
        let label = if running { "WORKING…" } else { "CONVERT 🔁" };
        let button = egui::Button::new(RichText::new(format!("\n{label}\n")));
        let response = ui.add_enabled_ui(!running, |ui| {
            ui.add_sized([ui.available_width(), 58.5], button)
        });
        if response.inner.clicked() {
            self.convert_in_thread(ctx);
        }
    }

    fn ui_controls(&mut self, ui: &mut egui::Ui) {
        let running = self.is_running.load(Ordering::Relaxed);
        ui.horizontal(|ui| {
            ui.label("Quality: ");
            ui.add_enabled(
                !self.lossless && !running,
                egui::Slider::new(&mut self.quality, 0.0..=100.0),
            );
            if ui.button("Clear log").clicked() {
                if let Ok(mut log) = self.log.lock() {
                    log.clear();
                }
            }
            ui.add_enabled_ui(!running, |ui| {
                ui.checkbox(&mut self.lossless, "Lossless");
            });
            ui.checkbox(&mut self.log_errors_only, "Show errors only");
        });
    }

    fn ui_progress(&self, ui: &mut egui::Ui) {
        let total = self.total.load(Ordering::Relaxed);
        let done = self.done.load(Ordering::Relaxed);
        let errors = self.errors.load(Ordering::Relaxed);
        let running = self.is_running.load(Ordering::Relaxed);
        if total == 0 && !running && done == 0 {
            return;
        }
        let frac = if total == 0 {
            0.0
        } else {
            (done as f32 / total as f32).clamp(0.0, 1.0)
        };
        let text = if total == 0 {
            "Scanning…".to_string()
        } else {
            format!("{done}/{total}  ✓ {} · ✗ {errors}", done.saturating_sub(errors))
        };
        ui.add(
            egui::ProgressBar::new(frac)
                .text(RichText::new(text).size(13.0))
                .desired_width(ui.available_width()),
        );
    }

    fn ui_log(&self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(Color32::from_rgb(20, 30, 40))
            .stroke(egui::Stroke::new(1.0, Color32::GRAY))
            .corner_radius(5)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let Ok(log) = self.log.lock() else { return };
                        for event in log.iter() {
                            if self.log_errors_only && !is_error_or_summary(event) {
                                continue;
                            }
                            let (text, color) = format_event(event);
                            ui.label(RichText::new(text).size(12.0).color(color));
                        }
                    });
            });
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_pending_pickers();

        let ctx = ui.ctx().clone();
        let panel_frame = egui::Frame::central_panel(&ctx.global_style())
            .inner_margin(egui::Margin::symmetric(12, 10));

        panel_frame.show(ui, |ui| {
            ui.heading("WebP Batch Converter App");
            ui.separator();

            ui.with_layout(
                egui::Layout::left_to_right(egui::Align::Min),
                |ui| {
                    let avail = ui.available_width();
                    let button_w = (avail * 0.22).clamp(110.0, 170.0);
                    let paths_w = avail - button_w - 8.0;
                    let row_h = 64.0;

                    ui.allocate_ui_with_layout(
                        egui::vec2(paths_w, row_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.ui_paths(ui, &ctx),
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(button_w, row_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            self.ui_convert_button(ui, &ctx);
                        },
                    );
                },
            );

            if let Some(err) = &self.validation_error {
                ui.colored_label(Color32::from_rgb(240, 120, 120), err);
            }

            ui.separator();
            self.ui_controls(ui);
            self.ui_progress(ui);
            ui.add_space(5.0);
            self.ui_log(ui);
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let serialized = format!(
            "input_path={}\noutput_path={}\nquality={}\nlossless={}\nlog_errors_only={}",
            self.input_path,
            self.output_path,
            self.quality,
            self.lossless,
            self.log_errors_only,
        );
        storage.set_string(STORAGE_KEY, serialized);
    }
}

fn format_event(event: &LogEvent) -> (String, Color32) {
    match event {
        LogEvent::Started { input_dir } => (
            format!("Starting conversion from: {input_dir}"),
            Color32::LIGHT_GRAY,
        ),
        LogEvent::Discovered { total } => (
            format!("Found {total} files to convert"),
            Color32::LIGHT_GRAY,
        ),
        LogEvent::Converted { path } => (
            format!("Converted: {}", path.display()),
            Color32::from_rgb(150, 220, 150),
        ),
        LogEvent::Skipped { path, .. } => (
            format!("Skipped (already exists): {}", path.display()),
            Color32::from_rgb(200, 180, 120),
        ),
        LogEvent::Error { msg } => (
            format!("Error: {msg}"),
            Color32::from_rgb(240, 120, 120),
        ),
        LogEvent::Finished { success, skipped, errors, total } => (
            format!("Finished — Success: {success}, Skipped: {skipped}, Errors: {errors}, Total: {total}"),
            Color32::LIGHT_BLUE,
        ),
    }
}

fn is_error_or_summary(event: &LogEvent) -> bool {
    matches!(
        event,
        LogEvent::Error { .. } | LogEvent::Finished { .. } | LogEvent::Discovered { .. }
    )
}
