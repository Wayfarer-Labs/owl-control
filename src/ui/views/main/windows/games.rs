use crate::config::{GameConfig, Preferences};
use constants::supported_games::{SupportedGame, SupportedGames};
use egui::{
    Align, Align2, Button, CollapsingHeader, Color32, Context, CursorIcon, Frame, Label, Layout,
    RichText, ScrollArea, Sense, Ui, Vec2, Window, vec2,
};

const FONTSIZE: f32 = 13.0;
const DEFAULT_WIDTH: f32 = 500.0;
const DEFAULT_HEIGHT: f32 = 600.0;

#[derive(Default)]
pub struct GamesWindowState {
    pub open: bool,
    pub installed_list: egui_virtual_list::VirtualList,
    pub uninstalled_list: egui_virtual_list::VirtualList,
    /// Currently open game settings window (stores the game name and primary exe)
    pub game_settings_open: Option<GameSettingsTarget>,
}
/// Identifies which game's settings window is open
#[derive(Clone)]
pub struct GameSettingsTarget {
    pub game_name: String,
    pub binaries: Vec<String>,
}

pub fn window(
    ctx: &Context,
    state: &mut GamesWindowState,
    supported_games: &SupportedGames,
    preferences: &mut Preferences,
) {
    // Always render the game settings window if it's open
    game_settings_window(ctx, &mut state.game_settings_open, preferences);

    if !state.open {
        return;
    }

    let (installed, uninstalled): (Vec<_>, Vec<_>) =
        supported_games.games.iter().partition(|g| g.installed);

    let mut should_close = false;
    let mut open_settings: Option<GameSettingsTarget> = None;

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
                                        let result = game_entry(ui, game, preferences);
                                        if result.launched {
                                            should_close = true;
                                        }
                                        if result.open_settings {
                                            open_settings = Some(GameSettingsTarget {
                                                game_name: game.game.clone(),
                                                binaries: game.binaries.clone(),
                                            });
                                        }
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
                                    let result = game_entry(ui, game, preferences);
                                    if result.launched {
                                        should_close = true;
                                    }
                                    if result.open_settings {
                                        open_settings = Some(GameSettingsTarget {
                                            game_name: game.game.clone(),
                                            binaries: game.binaries.clone(),
                                        });
                                    }
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

    if should_close {
        state.open = false;
    }

    if let Some(target) = open_settings {
        state.game_settings_open = Some(target);
    }
}

struct GameEntryResult {
    launched: bool,
    open_settings: bool,
}

fn game_entry(ui: &mut Ui, game: &SupportedGame, preferences: &Preferences) -> GameEntryResult {
    let alpha = if game.installed { 1.0 } else { 0.7 };
    let mut result = GameEntryResult {
        launched: false,
        open_settings: false,
    };

    // Check if any binary has custom settings
    let has_custom_settings = game
        .binaries
        .iter()
        .any(|exe| preferences.games.contains_key(exe));

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

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Launch button for installed games
                    if game.installed
                        && let Some(app_id) = game.steam_app_id
                    {
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
                            result.launched = true;
                        }
                    }

                    // Settings button
                    let settings_icon = if has_custom_settings { "⚙*" } else { "⚙" };
                    let settings_response = ui
                        .add_sized(
                            vec2(30.0, 20.0),
                            Button::new(
                                RichText::new(settings_icon)
                                    .size(FONTSIZE * 0.85)
                                    .color(ui.visuals().text_color().gamma_multiply(alpha)),
                            ),
                        )
                        .on_hover_text("Game-specific settings");
                    if settings_response.clicked() {
                        result.open_settings = true;
                    }
                });
            });
        });

    result
}

fn game_settings_window(
    ctx: &Context,
    game_settings_open: &mut Option<GameSettingsTarget>,
    preferences: &mut Preferences,
) {
    let Some(target) = game_settings_open.clone() else {
        return;
    };

    if target.binaries.is_empty() {
        return;
    }

    let hover_text = concat!(
        "Enable this if game capture doesn't work for this game.\n",
        "Window capture may have lower performance but better compatibility.\n",
        "NOTE: This will capture any overlays that render within the game window (Discord, Steam, etc) ",
        "- please turn these off."
    );

    let mut keep_open = true;
    Window::new(format!("{} Settings", target.game_name))
        .open(&mut keep_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                // Get the current config from the primary binary
                let mut config = get_primary_game_config(preferences, &target.binaries);

                let mut changed = false;

                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut config.use_window_capture, "Use Window Capture")
                        .changed()
                    {
                        changed = true;
                    }
                    ui.label(
                        RichText::new("(?)")
                            .size(12.0)
                            .color(Color32::from_rgb(150, 150, 150)),
                    )
                    .on_hover_text(hover_text);
                });

                // Sync to all binaries if anything changed
                if changed {
                    sync_game_config(preferences, &target.binaries, &config);
                }

                ui.add_space(8.0);

                // Reset button
                if ui
                    .add_sized(
                        vec2(ui.available_width(), 28.0),
                        Button::new(RichText::new("Reset to Defaults").size(12.0)),
                    )
                    .clicked()
                {
                    // Remove all config entries for this game's binaries
                    for exe in &target.binaries {
                        preferences.games.remove(exe);
                    }
                }
            });
        });

    if !keep_open {
        *game_settings_open = None;
    }
}

/// Get the game config for the primary binary (first in the list).
/// Returns a clone of the config to avoid borrow issues.
fn get_primary_game_config(preferences: &Preferences, binaries: &[String]) -> GameConfig {
    binaries
        .first()
        .and_then(|exe| preferences.games.get(exe))
        .cloned()
        .unwrap_or_default()
}

/// Sync game config across all binaries for a game.
/// Always writes to all binaries to ensure consistency.
fn sync_game_config(preferences: &mut Preferences, binaries: &[String], config: &GameConfig) {
    for exe in binaries {
        preferences.games.insert(exe.clone(), config.clone());
    }
}
