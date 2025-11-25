use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SupportedGame {
    pub game: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportedGames {
    pub games: Vec<SupportedGame>,
}

impl SupportedGames {
    pub fn load_from_str(s: &str) -> serde_json::Result<Self> {
        Ok(Self {
            games: serde_json::from_str(s)?,
        })
    }

    /// Do not use this unless you're sure you don't need a more up-to-date version.
    pub fn load_from_embedded() -> Self {
        Self::load_from_str(include_str!("supported_games.json"))
            .expect("Failed to load supported games from embedded data")
    }

    pub fn sort(&mut self) {
        self.games
            .sort_by(|a, b| a.game.to_lowercase().cmp(&b.game.to_lowercase()));
    }

    pub fn get(&self, game_name: &str) -> Option<&SupportedGame> {
        let game_name_lower = game_name.to_lowercase();
        self.games
            .iter()
            .find(|g| g.game.to_lowercase() == game_name_lower)
    }
}
