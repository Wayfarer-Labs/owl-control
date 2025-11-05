use egui::{Align, Button, CentralPanel, Color32, RichText, TextEdit, vec2};

use crate::{app_state::AsyncRequest, ui::views::App};

impl App {
    pub fn login_view(&mut self, ctx: &egui::Context) {
        CentralPanel::default().show(ctx, |ui| {
            // Center the content vertically and horizontally
            ui.vertical_centered(|ui| {
                // Extremely ugly bodge. I assume there's a way to do this correctly, but I can't find it at a glance.
                let content_height = 240.0;
                let available_height = ui.available_height();
                ui.add_space((available_height - content_height) / 2.0);

                ui.set_max_width(ui.available_width() * 0.8);
                ui.vertical_centered(|ui| {
                    // Logo/Icon area (placeholder for now)
                    ui.add_space(20.0);

                    // Main heading with better styling
                    ui.heading(
                        RichText::new("Welcome to OWL Control")
                            .size(28.0)
                            .strong()
                            .color(Color32::from_rgb(220, 220, 220)),
                    );

                    ui.add_space(8.0);

                    // Subtitle
                    ui.label(
                        RichText::new("Please enter your API key to continue")
                            .size(16.0)
                            .color(Color32::from_rgb(180, 180, 180)),
                    );

                    ui.add_space(20.0);

                    // API Key input section
                    ui.vertical_centered(|ui| {
                        // Styled text input
                        let text_edit = TextEdit::singleline(&mut self.login_api_key)
                            .desired_width(ui.available_width())
                            .vertical_align(Align::Center)
                            .hint_text("sk_...");

                        ui.add_sized(vec2(ui.available_width(), 40.0), text_edit);

                        ui.add_space(10.0);

                        // Help text
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                            ui.label(
                                RichText::new("Don't have an API key? Please sign up at ")
                                    .size(12.0)
                                    .color(Color32::from_rgb(140, 140, 140)),
                            );
                            ui.hyperlink_to(
                                RichText::new("our website.").size(12.0),
                                "https://wayfarerlabs.ai/handler/sign-up?after_auth_return_to=%2Fhandler%2Fsign-in",
                            );
                        });
                        ui.add_space(10.0);

                        if let Some(Err(err)) = &self.authenticated_user_id {
                            ui.label(
                                RichText::new(err)
                                    .size(12.0)
                                    .color(Color32::from_rgb(255, 0, 0)),
                            );
                            ui.add_space(10.0);
                        }

                        // Submit button
                        ui.add_enabled_ui(!self.is_authenticating_login_api_key, |ui| {
                            let submit_button = ui.add_sized(
                                vec2(120.0, 36.0),
                                Button::new(
                                    RichText::new(if self.is_authenticating_login_api_key {
                                        "Validating..."
                                    } else {
                                        "Continue"
                                    })
                                    .size(16.0)
                                    .strong(),
                                ),
                            );

                            if submit_button.clicked() && !self.is_authenticating_login_api_key {
                                self.is_authenticating_login_api_key = true;
                                self.app_state
                                    .async_request_tx
                                    .blocking_send(AsyncRequest::ValidateApiKey {
                                        api_key: self.login_api_key.clone(),
                                    })
                                    .ok();
                            }
                        });
                    });
                });
            });
        });
    }
}
