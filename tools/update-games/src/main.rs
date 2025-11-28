use constants::supported_games::SupportedGames;
use std::fs;

fn main() {
    let md_path = "GAMES.md";
    let marker_start = "<!-- MARKER:";

    let mut games = SupportedGames::load_from_embedded();
    games.sort();

    let md_content = fs::read_to_string(md_path).expect("Failed to read GAMES.md");

    let marker_start_pos = md_content
        .find(marker_start)
        .expect("Marker not found in GAMES.md");

    // Find the end of the marker (the closing -->)
    let marker_end_pos = md_content[marker_start_pos..]
        .find("-->")
        .expect("Marker end not found in GAMES.md")
        + marker_start_pos
        + 3; // +3 for "-->"

    let before_marker = &md_content[..marker_end_pos];

    let links: Vec<String> = games
        .games
        .iter()
        .map(|g| format!("- [{}]({})", g.game, g.url))
        .collect();

    let games_list = links.join("\n");

    let new_content = format!("{}\n\n{}\n", before_marker, games_list);

    fs::write(md_path, new_content).expect("Failed to write GAMES.md");

    println!("Updated GAMES.md with {} games", games.games.len());
}
