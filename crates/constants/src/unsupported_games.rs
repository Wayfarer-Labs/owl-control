pub struct UnsupportedGame {
    pub name: &'static str,
    pub binaries: &'static [&'static str],
    pub reason: &'static str,
}
const fn ug(
    name: &'static str,
    binaries: &'static [&'static str],
    reason: &'static str,
) -> UnsupportedGame {
    UnsupportedGame {
        name,
        binaries,
        reason,
    }
}

const ENOUGH_DATA_REASON: &str = "We have enough data for now.";

// -------------------------------------------------------------------
// AFTER UPDATING, `cargo run --bin update-unsupported-games` FOR DOCS
// -------------------------------------------------------------------
pub const UNSUPPORTED_GAMES: &[UnsupportedGame] = &[
    ug(
        "Minecraft",
        &[
            // Unfortunately, we can't easily detect Minecraft Java through this,
            // but I'm sure there's someone out there who will try Bedrock Edition
            "minecraft",
        ],
        ENOUGH_DATA_REASON,
    ),
    ug("Valorant", &["valorant-win64-shipping"], ENOUGH_DATA_REASON),
    ug("Counter-Strike: Source", &["cstrike"], ENOUGH_DATA_REASON),
    ug("Counter-Strike 2", &["cs2"], ENOUGH_DATA_REASON),
    ug(
        "Roblox",
        &["robloxstudiobeta", "robloxplayerbeta"],
        "Roblox recordings do not currently work.",
    ),
];
