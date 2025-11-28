use constants::supported_games::{SupportedGame, SupportedGames};
use egui::{
    Align, Button, CollapsingHeader, Color32, Context, CursorIcon, Frame, Label, Layout, RichText,
    ScrollArea, Sense, Ui, vec2,
};

const FONTSIZE: f32 = 13.0;
const DEFAULT_WIDTH: f32 = 500.0;
const DEFAULT_HEIGHT: f32 = 600.0;

#[derive(Default)]
pub struct GamesWindowState {
    pub open: bool,
    pub installed_list: egui_virtual_list::VirtualList,
    pub uninstalled_list: egui_virtual_list::VirtualList,
}

pub fn window(ctx: &Context, state: &mut GamesWindowState, supported_games: &SupportedGames) {
    if !state.open {
        return;
    }

    let (installed, uninstalled): (Vec<_>, Vec<_>) =
        supported_games.games.iter().partition(|g| g.installed);

    egui::Window::new("Games")
        .default_size([DEFAULT_WIDTH, DEFAULT_HEIGHT])
        .resizable(true)
        .open(&mut state.open)
        .show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                // Installed games section
                if !installed.is_empty() {
                    CollapsingHeader::new(RichText::new("Installed via Steam").size(14.0).strong())
                        .default_open(true)
                        .show(ui, |ui| {
                            state.installed_list.ui_custom_layout(
                                ui,
                                installed.len(),
                                |ui, index| {
                                    if let Some(game) = installed.get(index) {
                                        game_entry(ui, game);
                                        1
                                    } else {
                                        0
                                    }
                                },
                            );
                        });
                }

                // Uninstalled games section
                if !uninstalled.is_empty() {
                    CollapsingHeader::new(
                        RichText::new("Not installed via Steam").size(14.0).strong(),
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        state.uninstalled_list.ui_custom_layout(
                            ui,
                            uninstalled.len(),
                            |ui, index| {
                                if let Some(game) = uninstalled.get(index) {
                                    game_entry(ui, game);
                                    1
                                } else {
                                    0
                                }
                            },
                        );
                    });
                }
            });
        });
}

fn game_entry(ui: &mut Ui, game: &SupportedGame) {
    let alpha = if game.installed { 1.0 } else { 0.7 };

    Frame::new()
        .fill(ui.visuals().faint_bg_color.gamma_multiply(alpha))
        .inner_margin(4.0)
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // Game name (clickable - opens Steam store page)
                let game_response = ui
                    .add(
                        Label::new(
                            RichText::new(&game.game)
                                .size(FONTSIZE)
                                .color(ui.visuals().text_color().gamma_multiply(alpha))
                                .underline(),
                        )
                        .sense(Sense::click()),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text("Open Steam store page");
                if game_response.clicked() {
                    opener::open_browser(&game.url).ok();
                }

                // Launch button for installed games
                if !game.installed {
                    return;
                }
                let Some(app_id) = game.steam_app_id else {
                    return;
                };
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let response = ui
                        .add_sized(
                            vec2(60.0, 20.0),
                            Button::new(
                                RichText::new("Launch")
                                    .size(FONTSIZE * 0.85)
                                    .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(60, 120, 180)),
                        )
                        .on_hover_text("Launch game via Steam");
                    if response.clicked() {
                        let steam_launch_url = format!("steam://rungameid/{app_id}");
                        opener::open(&steam_launch_url).ok();
                    }
                });
            });
        });
}
