use super::*;
struct Version {
    texture: egui::TextureHandle,
    width: u32,
    height: u32,
}
pub(super) struct ImageView {
    before: Option<Version>,
    after: Option<Version>,
    mode: usize,
    blend: f32,
}
impl ImageView {
    fn new(ctx: &egui::Context, comparison: branchroom::image_diff::Comparison) -> Self {
        let upload = |name: &str, frame: branchroom::image_diff::Frame| {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [frame.width as usize, frame.height as usize],
                &frame.rgba,
            );
            Version {
                texture: ctx.load_texture(name, color, egui::TextureOptions::LINEAR),
                width: frame.width,
                height: frame.height,
            }
        };
        Self {
            before: comparison.before.map(|frame| upload("image_before", frame)),
            after: comparison.after.map(|frame| upload("image_after", frame)),
            mode: 0,
            blend: 0.5,
        }
    }
    fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.mode, 0, "Side by side");
            ui.selectable_value(&mut self.mode, 1, "Overlay");
            ui.selectable_value(&mut self.mode, 2, "Swipe");
        });
        ui.label(
            RichText::new("Image preview · animated files show the first frame")
                .size(12.)
                .color(MUTED()),
        );
        if self.mode == 0 {
            ui.columns(2, |cols| {
                for (index, version) in [&self.before, &self.after].into_iter().enumerate() {
                    cols[index]
                        .label(RichText::new(if index == 0 { "Before" } else { "After" }).strong());
                    if let Some(version) = version {
                        cols[index].label(format!("{} × {} pixels", version.width, version.height));
                        let max = egui::vec2(
                            cols[index].available_width(),
                            cols[index].available_height().clamp(150., 600.),
                        );
                        cols[index].add(
                            egui::Image::new((version.texture.id(), version.texture.size_vec2()))
                                .max_size(max)
                                .alt_text(if index == 0 {
                                    "Before image"
                                } else {
                                    "After image"
                                })
                                .bg_fill(SURFACE()),
                        );
                    } else {
                        cols[index].label(if index == 0 {
                            "File added"
                        } else {
                            "File deleted"
                        });
                    }
                }
            });
            return;
        }
        let (Some(before), Some(after)) = (&self.before, &self.after) else {
            subtitle(ui, "Use Side by side for an added or deleted image.");
            return;
        };
        ui.add(
            egui::Slider::new(&mut self.blend, 0.0..=1.0).text(if self.mode == 1 {
                "Overlay opacity"
            } else {
                "Swipe position"
            }),
        );
        let dimensions = egui::vec2(
            before.width.max(after.width) as f32,
            before.height.max(after.height) as f32,
        );
        let scale = (ui.available_width() / dimensions.x)
            .min(ui.available_height().clamp(150., 600.) / dimensions.y)
            .min(1.);
        let (rect, response) = ui.allocate_exact_size(dimensions * scale, egui::Sense::hover());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Image,
                ui.is_enabled(),
                if self.mode == 1 {
                    "Overlay image comparison"
                } else {
                    "Swipe image comparison"
                },
            )
        });
        ui.painter().rect_filled(rect, 0., SURFACE());
        let uv = egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.));
        let image_rect = |version: &Version| {
            egui::Rect::from_min_size(
                rect.min,
                egui::vec2(version.width as f32, version.height as f32) * scale,
            )
        };
        ui.painter().image(
            before.texture.id(),
            image_rect(before),
            uv,
            egui::Color32::WHITE,
        );
        if self.mode == 1 {
            ui.painter().image(
                after.texture.id(),
                image_rect(after),
                uv,
                egui::Color32::from_white_alpha((self.blend * 255.) as u8),
            );
        } else {
            let split = rect.left() + rect.width() * self.blend;
            ui.painter()
                .with_clip_rect(egui::Rect::from_min_max(
                    egui::pos2(split, rect.top()),
                    rect.max,
                ))
                .image(
                    after.texture.id(),
                    image_rect(after),
                    uv,
                    egui::Color32::WHITE,
                );
            ui.painter().line_segment(
                [
                    egui::pos2(split, rect.top()),
                    egui::pos2(split, rect.bottom()),
                ],
                egui::Stroke::new(2., ACCENT()),
            );
        }
    }
}
impl Branchroom {
    pub(super) fn image_comparison(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(comparison) = self.image_data.take() {
            self.image_view = Some(ImageView::new(ctx, comparison));
        }
        if let Some(view) = &mut self.image_view {
            view.show(ui);
        } else if ui
            .add_enabled(
                self.receiver.is_none(),
                egui::Button::new("Compare image versions"),
            )
            .clicked()
        {
            self.execute(
                Task::ImagePreview {
                    path: self.selected_path.clone(),
                    staged: self.diff_staged,
                },
                ctx,
            );
        }
    }
}
