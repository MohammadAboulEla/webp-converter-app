#![windows_subsystem = "windows"]

use eframe::NativeOptions;
use eframe::egui::{self, FontFamily, FontId, RichText, Style};
use rfd;
use std::sync::{Arc, Mutex};
use webp_converter_app::convert_to_webp_dir_threads;

fn main() -> Result<(), eframe::Error> {
    let window_width = 620.0;
    let window_height = 400.0;
    let window_x = 240.0;
    let window_y = 280.0;
    // min width
    let min_width = 565.0;
    let min_height = 280.0;
 
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([window_width, window_height])
            .with_position([window_x, window_y])
            .with_min_inner_size([min_width, min_height]),
        ..Default::default()
    };

    eframe::run_native(
        "WebP Converter App",
        options,
        Box::new(|cc| {
            // Clone the current style
            let mut style: Style = (*cc.egui_ctx.style()).clone();
            // Set the global font to 20.0 points, proportional family
            style.override_font_id = Some(FontId::new(20.0, FontFamily::Proportional));
            // Apply the modified style
            cc.egui_ctx.set_style(style);

            Box::new(MyApp::default())
        }),
    )
}

#[derive(Default)]
struct MyApp {
    input_path: String,
    output_path: String,
    quality: f32,
    lossless: bool,
    log: Arc<Mutex<String>>,
    log_errors_only: bool,
    initialized: bool,
}

impl MyApp {
    fn convert_in_thread(&self, ctx: &egui::Context) {
        let input = self.input_path.clone();
        let output = self.output_path.clone();
        let quality = self.quality;
        let lossless = self.lossless;
        let log = self.log.clone();

        // Clear previous log
        if let Ok(mut log) = self.log.lock() {
            log.clear();
        }

        let ctx = Arc::new(ctx.clone());

        std::thread::spawn(move || {
            let log_fn = {
                let log = log.clone();
                let ctx = ctx.clone();
                move |line: String| {
                    if let Ok(mut log) = log.lock() {
                        log.push_str(&line);
                        log.push('\n');
                    }
                    ctx.request_repaint();
                }
            };

            match convert_to_webp_dir_threads(&input, &output, quality, lossless, log_fn) {
                Ok(_) => {
                    // Final message is already handled by convert_to_webp_dir_threads
                    ctx.request_repaint();
                }
                Err(e) => {
                    if let Ok(mut log) = log.lock() {
                        log.push_str(&format!("Fatal error: {}\n", e));
                    }
                    ctx.request_repaint();
                }
            }
        });
    }

    fn select_input_directory(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.input_path = path.display().to_string();
        }
    }

    fn select_output_directory(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.output_path = path.display().to_string();
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.initialized {
                self.quality = 87.0;
                self.initialized = true;
                // self.update_log("Welcome to WebP Converter App!");
            }

            ui.heading("WebP Converter App");
            ui.separator();
            // Main container with 2 columns
            ui.horizontal(|ui| {
                let total_width = ui.available_width();
                let left_width = total_width * 0.8;
                let right_width = total_width * 0.2;

                ui.allocate_ui_with_layout(
                    egui::vec2(left_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        // Left column (wider for inputs)
                        ui.vertical(|ui| {
                            // Input Directory
                            ui.horizontal(|ui| {
                                ui.label("Input Directory:");
                                        // ui.add(egui::TextEdit::singleline(&mut self.input_path));
                                ui.add_sized(
                                    [ui.available_width() - 50.0, 30.0],
                                    egui::TextEdit::singleline(&mut self.input_path)
                                        .desired_width(ui.available_width() - 30.0),
                                );
                                let button =
                                    egui::Button::new("📁").min_size(egui::vec2(25.0, 25.0));
                                if ui.add(button).clicked() {
                                    self.select_input_directory();
                                }
                            });

                            // Output Directory
                            ui.horizontal(|ui| {
                                ui.label("Output Directory:");
                                ui.add_sized(
                                    [ui.available_width() - 50.0, 30.0],
                                    egui::TextEdit::singleline(&mut self.output_path)
                                        .desired_width(ui.available_width() - 30.0),
                                );
                                let button =
                                    egui::Button::new("📁").min_size(egui::vec2(25.0, 25.0));
                                if ui.add(button).clicked() {
                                    self.select_output_directory();
                                }
                            });
                        });
                    },
                );

                ui.allocate_ui_with_layout(
                    egui::vec2(right_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        // Right column (for convert button)
                        ui.add_space(5.0);
                        if ui
                            .add_sized(
                                [ui.available_width(), 30.0],
                                egui::Button::new(RichText::new("\nCONVERT 🔁\n")),
                            )
                            .clicked()
                        {
                            self.convert_in_thread(ctx);
                        }
                    },
                );
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Quality: ");
                // ui.add(egui::Slider::new(&mut self.quality, 0.0..=100.0));
                ui.add_enabled(
                    !self.lossless,
                    egui::Slider::new(&mut self.quality, 0.0..=100.0),
                );
                if ui.button("Clear log").clicked() {
                    if let Ok(mut log) = self.log.lock() {
                        log.clear();
                    }
                }
                ui.checkbox(&mut self.lossless, "Lossless");
                ui.checkbox(&mut self.log_errors_only, "Show errors only");
                if ui.button("Convert").clicked() {
                    self.convert_in_thread(ctx);
                }
            });
            ui.add_space(5.0);
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(20, 30, 40))
                .stroke(egui::Stroke::new(1.0, egui::Color32::GRAY))
                .rounding(5.0)
                .inner_margin(egui::style::Margin::same(8.0))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        // .max_height(220.0)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            if let Ok(log) = self.log.lock() {
                                if self.log_errors_only {
                                    for line in log.lines().filter(|l| !l.starts_with("Converted"))
                                    {
                                        ui.label(RichText::new(line).size(12.0));
                                    }
                                } else {
                                    // ui.label(log.as_str());
                                    ui.label(RichText::new(log.as_str()).size(12.0));
                                }
                            }
                        });
                });
        });
    }
}
