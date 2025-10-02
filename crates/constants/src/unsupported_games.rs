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

pub const ENOUGH_DATA_REASON: &str = "We have enough data for now.";

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
    ug("Overwatch 2", &["overwatch"], ENOUGH_DATA_REASON),
    ug("Team Fortress 2", &["tf", "tf_win64"], ENOUGH_DATA_REASON),
    ug("Apex Legends", &["r5apex"], ENOUGH_DATA_REASON),
    ug("Rainbow Six Siege", &["rainbowsix"], ENOUGH_DATA_REASON),
    ug("Squad", &["squad"], ENOUGH_DATA_REASON),
    ug(
        "Hell Let Loose",
        &["hll-win64-shipping"],
        ENOUGH_DATA_REASON,
    ),
    ug("GTA III", &["gta3"], ENOUGH_DATA_REASON),
    ug("GTA: Vice City", &["gta-vc"], ENOUGH_DATA_REASON),
    ug("GTA: San Andreas", &["gta_sa"], ENOUGH_DATA_REASON),
    ug("GTA IV", &["gtaiv"], ENOUGH_DATA_REASON),
    ug("GTA V", &["gta5"], ENOUGH_DATA_REASON),
    ug("Subnautica", &["subnautica"], ENOUGH_DATA_REASON),
    ug("Far Cry", &["farcry"], ENOUGH_DATA_REASON),
    ug("Far Cry 2", &["farcry2"], ENOUGH_DATA_REASON),
    ug("Far Cry 3", &["farcry3"], ENOUGH_DATA_REASON),
    ug("Far Cry 4", &["farcry4"], ENOUGH_DATA_REASON),
    ug("Far Cry 5", &["farcry5"], ENOUGH_DATA_REASON),
    ug("Far Cry 6", &["farcry6"], ENOUGH_DATA_REASON),
    ug(
        "Roblox",
        &["robloxstudiobeta", "robloxplayerbeta"],
        "Recorded footage is all-black.",
    ),
    ug(
        "Destiny 2",
        &["destiny2launcher", "destiny2"],
        "Recorded footage is all-black.",
    ),
    ug(
        "Split Fiction",
        &["splitfiction"],
        "Split-screen games are unsupported.",
    ),
];
