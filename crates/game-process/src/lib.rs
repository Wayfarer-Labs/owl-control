use std::{
    ffi::{CStr, CString, OsString},
    path::PathBuf,
};

use color_eyre::Result;

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, RECT, STILL_ACTIVE},
        Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromWindow},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_NAME_NATIVE,
                PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameA,
            },
            WindowsProgramming::HW_PROFILE_INFOA,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    },
    core::{Error, PSTR},
};

pub use windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub u32);

struct CloseProcessOnDrop(HANDLE);

impl Drop for CloseProcessOnDrop {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0).unwrap();
        }
    }
}

pub fn does_process_exist(Pid(pid): Pid) -> Result<bool, Error> {
    unsafe {
        let process =
            CloseProcessOnDrop(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?);
        let mut exit_code = 0;
        GetExitCodeProcess(process.0, &mut exit_code)?;
        Ok(exit_code == STILL_ACTIVE.0 as u32)
    }
}

pub fn exe_name_for_pid(Pid(pid): Pid) -> Result<PathBuf> {
    unsafe {
        let process =
            CloseProcessOnDrop(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?);

        let mut process_name = [0; 256];
        let mut process_name_size = process_name.len() as u32;
        QueryFullProcessImageNameA(
            process.0,
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
            return Err(Error::from_win32());
        }
        Ok((hwnd, Pid(pid)))
    }
}

pub fn is_window_fullscreen(hwnd: HWND) -> Result<bool> {
    unsafe {
        let mut window_rect = RECT::default();
        windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut window_rect)?;

        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let mut monitor_info = windows::Win32::Graphics::Gdi::MONITORINFO {
            cbSize: std::mem::size_of::<windows::Win32::Graphics::Gdi::MONITORINFO>() as u32,
            ..Default::default()
        };
        windows::Win32::Graphics::Gdi::GetMonitorInfoA(monitor, &mut monitor_info).ok()?;
        let monitor_rect = monitor_info.rcMonitor;

        Ok(window_rect == monitor_rect)
    }
}

struct CloseHandleOnDrop(HANDLE);

impl Drop for CloseHandleOnDrop {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0).unwrap();
        }
    }
}

pub fn iter_processes() -> Result<impl Iterator<Item = PROCESSENTRY32>, Error> {
    unsafe {
        let snapshot = CloseHandleOnDrop(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?);

        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..Default::default()
        };

        let mut result = Process32First(snapshot.0, &mut entry).ok().map(|()| entry);

        Ok(std::iter::from_fn(move || {
            let r = result?;
            result = Process32Next(snapshot.0, &mut entry).ok().map(|()| entry);
            Some(r)
        }))
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
