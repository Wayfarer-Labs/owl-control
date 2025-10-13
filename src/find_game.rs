use color_eyre::{Result, eyre::OptionExt};
use game_process::{Pid, exe_name_for_pid, foreground_window, windows::Win32::Foundation::HWND};

pub(crate) fn get_foregrounded_game() -> Result<Option<(String, Pid, HWND)>> {
    let (hwnd, pid) = foreground_window()?;

    let exe_path = exe_name_for_pid(pid)?;
    let exe_name = exe_path
        .file_name()
        .ok_or_eyre("Failed to get file name from exe path")?
        .to_str()
        .ok_or_eyre("Failed to convert exe name to unicode string")?
        .to_owned();

    Ok(Some((exe_name, pid, hwnd)))
}
