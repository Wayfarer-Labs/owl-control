use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnsupportedGame {
    pub name: String,
    pub binaries: Vec<String>,
    pub reason: UnsupportedGameReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnsupportedGameReason {
    EnoughData,
    NotAGame,
    Other(String),
    #[serde(untagged)]
    Unknown(String),
}
impl std::fmt::Display for UnsupportedGameReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnsupportedGameReason::EnoughData => write!(f, "We have enough data for now."),
            UnsupportedGameReason::NotAGame => write!(f, "This is not a game."),
            UnsupportedGameReason::Other(s) => write!(f, "{s}"),
            UnsupportedGameReason::Unknown(s) => write!(
                f,
                "Unknown reason: {s} (please update your version of OWL Control)"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnsupportedGames {
    pub games: Vec<UnsupportedGame>,
}
impl UnsupportedGames {
    pub fn load_from_str(s: &str) -> serde_json::Result<Self> {
        Ok(Self {
            games: serde_json::from_str(s)?,
        })
    }

    /// Do not use this unless you're sure you don't need a more up-to-date version.
    pub fn load_from_embedded() -> Self {
        Self::load_from_str(include_str!("unsupported_games.json"))
            .expect("Failed to load unsupported games from embedded data")
    }

    pub fn sort(&mut self) {
        self.games.sort_by(|a, b| a.name.cmp(&b.name));
    }

    pub fn get(&self, game_exe_without_extension_lowercase: String) -> Option<&UnsupportedGame> {
        // TODO: optimize with exe->&(reason, name) hashmap
        self.games
            .iter()
            .find(|ug| ug.binaries.contains(&game_exe_without_extension_lowercase))
    }
}
