use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SupportedGame {
    pub game: String,
    pub url: String,
    pub binaries: Vec<String>,
    pub steam_app_id: Option<u32>,
    pub installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportedGames {
    pub games: Vec<SupportedGame>,
}

impl SupportedGames {
    pub fn load_from_str(s: &str) -> serde_json::Result<Self> {
        /// Internal struct for JSON deserialization
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct RawSupportedGame {
            game: String,
            url: String,
            binaries: Vec<String>,
        }

        let raw_games: Vec<RawSupportedGame> = serde_json::from_str(s)?;
        let installed_app_ids = detect_installed_app_ids();

        let games = raw_games
            .into_iter()
            .map(|raw| {
                let steam_app_id = extract_steam_app_id(&raw.url);
                let installed = steam_app_id.is_some_and(|id| installed_app_ids.contains(&id));
                SupportedGame {
                    game: raw.game,
                    url: raw.url,
                    binaries: raw.binaries,
                    steam_app_id,
                    installed,
                }
            })
            .collect();

        Ok(Self { games })
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

    pub fn get(&self, game_exe_without_ext: &str) -> Option<&SupportedGame> {
        let game_exe_without_ext = game_exe_without_ext.to_lowercase();
        self.games.iter().find(|g| {
            g.binaries.iter().any(|b| {
                let b_lower = b.to_lowercase();
                // Exact match or exe has a suffix (e.g., _dx12, -win64-shipping)
                game_exe_without_ext == b_lower
                    || game_exe_without_ext.starts_with(&format!("{b_lower}_"))
                    || game_exe_without_ext.starts_with(&format!("{b_lower}-"))
            })
        })
    }

    pub fn installed(&self) -> impl Iterator<Item = &SupportedGame> {
        self.games.iter().filter(|g| g.installed)
    }

    pub fn uninstalled(&self) -> impl Iterator<Item = &SupportedGame> {
        self.games.iter().filter(|g| !g.installed)
    }
}

fn extract_steam_app_id(url: &str) -> Option<u32> {
    // Parse "https://store.steampowered.com/app/278360/..." -> Some(278360)
    url.strip_prefix("https://store.steampowered.com/app/")?
        .split('/')
        .next()?
        .parse()
        .ok()
}

fn detect_installed_app_ids() -> Vec<u32> {
    let Ok(steam_dir) = steamlocate::SteamDir::locate() else {
        tracing::warn!("Steam installation not found");
        return vec![];
    };

    let Ok(libraries) = steam_dir.libraries() else {
        tracing::warn!("Failed to read Steam libraries");
        return vec![];
    };

    let mut installed = vec![];
    for lib in libraries {
        let Ok(library) = lib else {
            tracing::warn!("Failed to read Steam library");
            continue;
        };
        for app in library.apps() {
            let Ok(app) = app else {
                tracing::warn!("Failed to read app");
                continue;
            };
            installed.push(app.app_id);
        }
    }
    installed
}
