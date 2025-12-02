use std::{
    ffi::{CStr, CString, OsString},
    path::PathBuf,
};

use color_eyre::{Result, eyre::Context as _};

use windows::{
    Win32::{
        Foundation::{HWND, STILL_ACTIVE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, MODULEENTRY32, Module32First, Module32Next,
                PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPMODULE,
                TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
            },
            Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_NAME_NATIVE, PROCESS_QUERY_INFORMATION,
                PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameA,
            },
            WindowsProgramming::HW_PROFILE_INFOA,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    },
    core::{Error, Owned, PSTR},
};

pub use windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub u32);

pub fn does_process_exist(Pid(pid): Pid) -> Result<bool, Error> {
    unsafe {
        let process = Owned::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?);
        let mut exit_code = 0;
        GetExitCodeProcess(*process, &mut exit_code)?;
        Ok(exit_code == STILL_ACTIVE.0 as u32)
    }
}

pub fn exe_name_for_pid(Pid(pid): Pid) -> Result<PathBuf> {
    unsafe {
        let process = Owned::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?);

        let mut process_name = [0; 256];
        let mut process_name_size = process_name.len() as u32;
        QueryFullProcessImageNameA(
            *process,
            PROCESS_NAME_NATIVE,
            PSTR(&mut process_name as *mut u8),
            &mut process_name_size,
        )?;
        let process_name = CString::new(&process_name[..process_name_size.try_into().unwrap()])?;
        let process_name = OsString::from_encoded_bytes_unchecked(process_name.into_bytes());
        let process_name = PathBuf::from(process_name);
        Ok(process_name)
    }
}

pub fn foreground_window() -> Result<(HWND, Pid), Error> {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        if GetWindowThreadProcessId(hwnd, Some(&mut pid)) == 0 {
            return Err(Error::from_thread());
        }
        Ok((hwnd, Pid(pid)))
    }
}

pub fn for_each_process(mut f: impl FnMut(PROCESSENTRY32) -> bool) -> Result<(), Error> {
    unsafe {
        let snapshot = Owned::new(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?);

        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..Default::default()
        };

        if Process32First(*snapshot, &mut entry).is_err() {
            return Ok(());
        }

        loop {
            if !f(entry) {
                break;
            }
            if Process32Next(*snapshot, &mut entry).is_err() {
                break;
            }
        }

        Ok(())
    }
}

pub fn hardware_id() -> Result<String> {
    unsafe {
        let mut hw_profile_info = HW_PROFILE_INFOA::default();

        windows::Win32::System::WindowsProgramming::GetCurrentHwProfileA(&mut hw_profile_info)?;

        let guid = hw_profile_info.szHwProfileGuid.map(|x| x as u8);
        let guid = CStr::from_bytes_with_nul(&guid)?;
        Ok(guid.to_str()?.to_owned())
    }
}

/// Gets all of the modules loaded by the process.
pub fn get_modules(pid: Pid) -> Result<Vec<String>> {
    unsafe {
        // Open the target process with query permissions
        let process_handle = OpenProcess(PROCESS_QUERY_INFORMATION, false, pid.0)
            .context("Failed to open process")?;

        let _process_guard = Owned::new(process_handle);

        // Create a snapshot of all modules (DLLs) loaded by the process
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid.0)
            .context("Failed to create module snapshot")?;
        let _snapshot_guard = Owned::new(snapshot);

        let mut module_entry = MODULEENTRY32 {
            dwSize: std::mem::size_of::<MODULEENTRY32>() as u32,
            ..Default::default()
        };

        // Get the first module
        if Module32First(snapshot, &mut module_entry).is_err() {
            return Ok(vec![]);
        }

        let mut output = vec![];

        // Check all loaded modules for graphics API DLLs
        loop {
            output.push(
                std::ffi::CStr::from_ptr(module_entry.szModule.as_ptr())
                    .to_string_lossy()
                    .to_string(),
            );

            // Move to next module
            if Module32Next(snapshot, &mut module_entry).is_err() {
                break;
            }
        }

        Ok(output)
    }
}
