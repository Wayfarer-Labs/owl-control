use constants::supported_games::{SupportedGame, SupportedGames};
use egui::{Context, CursorIcon, Frame, Label, RichText, ScrollArea, Sense, Ui};

const FONTSIZE: f32 = 13.0;
const DEFAULT_WIDTH: f32 = 500.0;
const DEFAULT_HEIGHT: f32 = 600.0;

#[derive(Default)]
pub struct GamesWindowState {
    pub open: bool,
    pub virtual_list: egui_virtual_list::VirtualList,
}

pub fn window(ctx: &Context, state: &mut GamesWindowState, supported_games: &SupportedGames) {
    if !state.open {
        return;
    }

    let games = supported_games.games.clone();
    let game_count = games.len();

    egui::Window::new("Games")
        .default_size([DEFAULT_WIDTH, DEFAULT_HEIGHT])
        .resizable(true)
        .open(&mut state.open)
        .show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                state
                    .virtual_list
                    .ui_custom_layout(ui, game_count, |ui, index| {
                        if let Some(game) = games.get(index) {
                            game_entry(ui, game);
                            1
                        } else {
                            0
                        }
                    });
            });
        });
}

fn game_entry(ui: &mut Ui, game: &SupportedGame) {
    Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(4.0)
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui
                    .add(
                        Label::new(RichText::new(&game.game).size(FONTSIZE).underline())
                            .sense(Sense::click()),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    opener::open_browser(&game.url).ok();
                }
            });
        });
}
