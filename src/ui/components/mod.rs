use egui::{
    Color32, Label, TextFormat, TextStyle, TextWrapMode,
    text::{LayoutJob, TextWrapping},
};

use crate::app_state::ForegroundedGame;

pub fn foregrounded_game(
    ui: &mut egui::Ui,
    foregrounded_game: Option<&ForegroundedGame>,
    font_size: Option<f32>,
) {
    let mut body_font = TextStyle::Body.resolve(ui.style());
    let mut monospace_font = TextStyle::Monospace.resolve(ui.style());
    if let Some(font_size) = font_size {
        body_font.size = font_size;
        monospace_font.size = font_size;
    }

    let mut job = LayoutJob::default();
    if let Some(fg) = foregrounded_game {
        let (color, icon) = if fg.is_recordable() {
            (Color32::from_rgb(100, 255, 100), "✅")
        } else {
            (Color32::from_rgb(255, 100, 100), "❌")
        };

        job.append(
            icon,
            0.0,
            TextFormat {
                font_id: body_font.clone(),
                color,
                ..Default::default()
            },
        );
        job.append(
            fg.exe_name.as_deref().unwrap_or("Unknown"),
            4.0,
            TextFormat {
                font_id: monospace_font,
                color,
                ..Default::default()
            },
        );
        if !fg.is_recordable() {
            job.append(
                "(unsupported)",
                4.0,
                TextFormat {
                    font_id: body_font.clone(),
                    color: Color32::from_rgb(200, 100, 100),
                    ..Default::default()
                },
            );
        }
    } else {
        job.append(
            "No window detected",
            0.0,
            TextFormat {
                font_id: monospace_font,
                color: Color32::from_rgb(150, 150, 150),
                ..Default::default()
            },
        );
    }
    job.wrap = TextWrapping::no_max_width();

    let r = ui.add(Label::new(job).wrap_mode(TextWrapMode::Extend));
    if let Some(reason) = foregrounded_game.and_then(|fg| fg.unsupported_reason.as_ref()) {
        r.on_hover_text(reason);
    }
}
