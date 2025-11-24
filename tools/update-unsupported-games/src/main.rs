use constants::unsupported_games::{UnsupportedGameReason, UnsupportedGames};
use std::fs;

fn main() {
    // Read the current file
    let path = "GAMES.md";
    let content = fs::read_to_string(path).unwrap_or_else(|_| panic!("Failed to read {path}"));

    let mut unsupported_games = UnsupportedGames::load_from_embedded();
    unsupported_games.sort();

    // Find the position of "# Unwanted games"
    let Some(pos) = content.find("# Unwanted games") else {
        eprintln!("Could not find '# Unwanted games' section in {path}");
        return;
    };

    // Generate the unwanted games section
    let mut output = String::new();
    output.push_str("# Unwanted games\n\n");
    output.push_str("<!-- This list is sourced from `crates/constants/src/unsupported_games.json`. If you update that file, please run `cargo run -p update-unsupported-games` to update this list. -->\n\n");
    output.push_str("We have already collected sufficient data for these games, or they are not supported by OWL Control.\n");
    output.push_str("Any data submitted for these games will be rejected by our system.\n");
    output.push_str("Please do not submit data for these games.\n\n");

    output.push_str("## Banned games / Sufficient data captured\n\n");
    for game in unsupported_games
        .games
        .iter()
        .filter(|game| game.reason == UnsupportedGameReason::EnoughData)
    {
        output.push_str(&format!("- {}\n", game.name));
    }
    output.push('\n');

    output.push_str("## Unsupported games\n\n");
    for game in unsupported_games.games.iter().filter(|game| {
        ![
            UnsupportedGameReason::EnoughData,
            UnsupportedGameReason::NotAGame,
        ]
        .contains(&game.reason)
    }) {
        output.push_str(&format!("- {}: {}\n", game.name, game.reason));
    }

    // Update the content
    let updated_content = format!("{}{}\n", &content[..pos], output.trim());
    fs::write(path, updated_content).unwrap_or_else(|_| panic!("Failed to write {path}"));

    println!("Updated {path} with unsupported games list");
}
