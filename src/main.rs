use eframe::egui;
use resvg::tiny_skia;
use usvg::{fontdb, Options, Tree};

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "SVG-SCAD Studio",
        native_options,
        Box::new(|_cc| Box::new(SvgApp::default())),
    )
}

struct SvgApp {
    code: String,
    texture: Option<egui::TextureHandle>,
    error_msg: Option<String>,
    font_db: fontdb::Database,
    // Pan & Zoom controls
    zoom: f32,
    pan: egui::Vec2,
    // Cache current rendered pixel buffer for export
    current_pixmap: Option<tiny_skia::Pixmap>,
}

impl Default for SvgApp {
    fn default() -> Self {
        let mut font_db = fontdb::Database::new();
        font_db.load_system_fonts();

        Self {
            code: r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="100%" height="100%">
  <defs>
    <linearGradient id="grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:#4F46E5;stop-opacity:1" />
      <stop offset="100%" style="stop-color:#06B6D4;stop-opacity:1" />
    </linearGradient>
  </defs>
  <circle cx="100" cy="100" r="80" fill="url(#grad)" />
  <rect x="60" y="60" width="80" height="80" fill="#F59E0B" rx="10" />
</svg>"##
                .to_string(),
            texture: None,
            error_msg: None,
            font_db,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            current_pixmap: None,
        }
    }
}

impl SvgApp {
    /// Renders full raw XML/SVG string into an egui texture and caches the pixmap
    fn update_texture(&mut self, ctx: &egui::Context) {
        let opt = Options::default();

        // Parses complete raw XML/SVG document directly
        match Tree::from_str(&self.code, &opt, &self.font_db) {
            Ok(tree) => {
                let size = tree.size().to_int_size();
                let width = size.width().max(1);
                let height = size.height().max(1);

                let mut pixmap = tiny_skia::Pixmap::new(width, height)
                    .unwrap_or_else(|| tiny_skia::Pixmap::new(100, 100).unwrap());

                resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [pixmap.width() as usize, pixmap.height() as usize],
                    pixmap.data(),
                );

                self.texture = Some(ctx.load_texture(
                    "svg_preview",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ));
                self.current_pixmap = Some(pixmap);
                self.error_msg = None;
            }
            Err(err) => {
                self.error_msg = Some(err.to_string());
            }
        }
    }

    /// Reset view coordinates
    fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
    }
}

impl eframe::App for SvgApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Initial draw render
        if self.texture.is_none() && self.error_msg.is_none() {
            self.update_texture(ctx);
        }

        // --- TOP TOOLBAR ---
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("⚡ SVG-SCAD Studio");
                ui.separator();

                // EXPORT SVG BUTTON
                if ui.button("💾 Export SVG").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("SVG Vector Image", &["svg"])
                        .set_file_name("render.svg")
                        .save_file()
                    {
                        if let Err(e) = std::fs::write(&path, &self.code) {
                            eprintln!("Failed to save SVG file: {}", e);
                        }
                    }
                }

                // EXPORT PNG BUTTON
                if ui.button("🖼️ Export PNG").clicked() {
                    if let Some(pixmap) = &self.current_pixmap {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("PNG Image", &["png"])
                            .set_file_name("render.png")
                            .save_file()
                        {
                            let _ = pixmap.save_png(path);
                        }
                    }
                }

                ui.separator();
                ui.label(format!("Zoom: {:.0}%", self.zoom * 100.0));
                if ui.button("🔍 Reset View").clicked() {
                    self.reset_view();
                }
            });
        });

        // --- BOTTOM STATUS BAR ---
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.small("Tip: Use Mouse Wheel to Zoom, Right-Click Drag to Pan.");
            });
        });

        // --- MAIN WORKSPACE ---
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                // LEFT PANEL: Code Editor
                columns[0].vertical(|ui| {
                    ui.heading("Full XML / SVG Source");

                    let text_response = ui.add(
                        egui::TextEdit::multiline(&mut self.code)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .desired_rows(35),
                    );

                    if text_response.changed() {
                        self.update_texture(ctx);
                    }

                    if let Some(err) = &self.error_msg {
                        ui.add_space(8.0);
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("⚠️ XML / SVG Syntax Error:\n{}", err),
                        );
                    }
                });

                // RIGHT PANEL: Expanded Interactive Viewport
                columns[1].vertical(|ui| {
                    ui.heading("Viewport");

                    // Expand viewport area to fill all available space in panel
                    let available_space = ui.available_size();
                    let (response, painter) = ui.allocate_painter(
                        available_space,
                        egui::Sense::drag(),
                    );

                    // Draw Background for Transparency
                    let rect = response.rect;
                    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));

                    if let Some(texture) = &self.texture {
                        // --- INTERACTION HANDLING ---

                        // Handle Pan via dragging
                        if response.dragged_by(egui::PointerButton::Secondary)
                            || response.dragged_by(egui::PointerButton::Middle)
                            || response.dragged_by(egui::PointerButton::Primary)
                        {
                            self.pan += response.drag_delta();
                        }

                        // Handle Zoom via scroll wheel over viewport
                        if response.hovered() {
                            let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);
                            if scroll_delta != 0.0 {
                                let zoom_factor = if scroll_delta > 0.0 { 1.1 } else { 0.9 };
                                self.zoom = (self.zoom * zoom_factor).clamp(0.1, 20.0);
                            }
                        }

                        // --- RENDERING ---
                        let img_size = texture.size_vec2() * self.zoom;
                        let center_pos = rect.center() + self.pan - (img_size / 2.0);

                        let image_rect = egui::Rect::from_min_size(center_pos, img_size);

                        // Draw white background canvas behind SVG
                        painter.rect_filled(image_rect, 4.0, egui::Color32::WHITE);

                        // Paint SVG texture onto viewport
                        painter.image(
                            texture.id(),
                            image_rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                });
            });
        });
    }
}
