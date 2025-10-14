use crate::ui::{HEADING_TEXT_SIZE, MainApp, SUBHEADING_TEXT_SIZE};

impl MainApp {
    pub fn consent_view(&mut self, ctx: &egui::Context) {
        let padding = 8;
        let button_font_size = 14.0;

        egui::TopBottomPanel::top("consent_panel_top").show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(padding))
                .show(ui, |ui| {
                    ui.heading(
                        egui::RichText::new("Informed Consent & Terms of Service")
                            .size(HEADING_TEXT_SIZE)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new("Please read the following information carefully.")
                            .size(SUBHEADING_TEXT_SIZE),
                    );
                });
        });

        egui::TopBottomPanel::bottom("consent_panel_bottom").show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(padding))
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(8.0, 2.0);
                            if ui
                                .add_enabled(
                                    self.has_scrolled_to_bottom_of_consent,
                                    egui::Button::new(
                                        egui::RichText::new("Accept")
                                            .size(button_font_size)
                                            .strong(),
                                    ),
                                )
                                .clicked()
                            {
                                self.go_to_main();
                            }
                            if ui
                                .button(
                                    egui::RichText::new("Cancel")
                                        .size(button_font_size)
                                        .strong(),
                                )
                                .clicked()
                            {
                                self.go_to_login();
                            }
                        });
                    });
                });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(padding))
                .show(ui, |ui| {
                    let output = egui::ScrollArea::vertical().show(ui, |ui| {
                        egui_commonmark::commonmark_str!(
                            ui,
                            &mut self.md_cache,
                            "./src/ui/consent.md"
                        );
                    });

                    self.has_scrolled_to_bottom_of_consent |= (output.state.offset.y
                        + output.inner_rect.height())
                        >= output.content_size.y;
                });
        });
    }
}
