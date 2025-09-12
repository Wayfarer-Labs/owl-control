use constants::unsupported_games::UNSUPPORTED_GAMES;
use std::fs;

fn main() {
    // Read the current file
    let path = "GAMES.md";
    let content = fs::read_to_string(path).unwrap_or_else(|_| panic!("Failed to read {path}"));

    // Find the position of "# Unwanted games"
    let Some(pos) = content.find("# Unwanted games") else {
        eprintln!("Could not find '# Unwanted games' section in {path}");
        return;
    };

    // Generate the unwanted games section
    let mut output = String::new();
    output.push_str("# Unwanted games\n\n");
    output.push_str("We have already collected sufficient data for these games, or they are not supported by OWL Control.\n");
    output.push_str("Any data submitted for these games will be rejected by our system.\n");
    output.push_str("Please do not submit data for these games.\n\n");
    for game in UNSUPPORTED_GAMES {
        output.push_str(&format!("- {}\n", game.name));
    }

    // Update the content
    let updated_content = format!("{}{}\n", &content[..pos], output.trim());
    fs::write(path, updated_content).unwrap_or_else(|_| panic!("Failed to write {path}"));

    println!("Updated {path} with unsupported games list");
}
