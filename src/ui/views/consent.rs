use egui::{
    Align, Button, CentralPanel, Frame, Layout, Margin, RichText, ScrollArea, TopBottomPanel, vec2,
};

use crate::ui::views::{App, HEADING_TEXT_SIZE, SUBHEADING_TEXT_SIZE};

impl App {
    pub fn consent_view(&mut self, ctx: &egui::Context) {
        let padding = 8;
        let button_font_size = 14.0;

        TopBottomPanel::top("consent_panel_top").show(ctx, |ui| {
            Frame::new()
                .inner_margin(Margin::same(padding))
                .show(ui, |ui| {
                    ui.heading(
                        RichText::new("Informed Consent & Terms of Service")
                            .size(HEADING_TEXT_SIZE)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("Please read the following information carefully.")
                            .size(SUBHEADING_TEXT_SIZE),
                    );
                });
        });

        TopBottomPanel::bottom("consent_panel_bottom").show(ctx, |ui| {
            Frame::new()
                .inner_margin(Margin::same(padding))
                .show(ui, |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().button_padding = vec2(8.0, 2.0);
                            if ui
                                .add_enabled(
                                    self.has_scrolled_to_bottom_of_consent,
                                    Button::new(
                                        RichText::new("Accept").size(button_font_size).strong(),
                                    ),
                                )
                                .clicked()
                            {
                                self.go_to_main();
                            }
                            if ui
                                .button(RichText::new("Cancel").size(button_font_size).strong())
                                .clicked()
                            {
                                self.go_to_login();
                            }
                        });
                    });
                });
        });

        CentralPanel::default().show(ctx, |ui| {
            Frame::new()
                .inner_margin(Margin::same(padding))
                .show(ui, |ui| {
                    let output = ScrollArea::vertical().show(ui, |ui| {
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
