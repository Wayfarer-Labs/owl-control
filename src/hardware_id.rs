use color_eyre::Result;
use game_process::hardware_id;

pub(crate) fn get() -> Result<String> {
    // Strip "{}" off the ends of the windows hardware ID
    hardware_id().map(|id| id[1..id.len() - 1].to_owned())
}
