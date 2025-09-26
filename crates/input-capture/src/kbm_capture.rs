use color_eyre::{
    Result,
    eyre::{Context, bail},
};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleA,
        UI::{
            Input::{
                self, GetRawInputData, HRAWINPUT,
                KeyboardAndMouse::{VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2},
                MOUSE_MOVE_ABSOLUTE, MOUSE_VIRTUAL_DESKTOP, RAWINPUT, RAWINPUTDEVICE,
                RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RegisterRawInputDevices,
            },
            WindowsAndMessaging::{
                self, CreateWindowExA, DefWindowProcA, DestroyWindow, DispatchMessageA,
                GetMessageA, GetSystemMetrics, HWND_MESSAGE, MSG, PostQuitMessage, RI_KEY_BREAK,
                RI_MOUSE_BUTTON_4_DOWN, RI_MOUSE_BUTTON_4_UP, RI_MOUSE_BUTTON_5_DOWN,
                RI_MOUSE_BUTTON_5_UP, RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_LEFT_BUTTON_UP,
                RI_MOUSE_MIDDLE_BUTTON_DOWN, RI_MOUSE_MIDDLE_BUTTON_UP, RI_MOUSE_RIGHT_BUTTON_DOWN,
                RI_MOUSE_RIGHT_BUTTON_UP, RI_MOUSE_WHEEL, RegisterClassA, SM_CXSCREEN,
                SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
                SM_YVIRTUALSCREEN, TranslateMessage, UnregisterClassA, WINDOW_EX_STYLE,
                WINDOW_STYLE, WNDCLASSA,
            },
        },
    },
    core::PCSTR,
};

use crate::{Event, PressState};

pub struct KbmCapture {
    hwnd: HWND,
    class_name: PCSTR,
    h_instance: HINSTANCE,
}
impl Drop for KbmCapture {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd).expect("failed to destroy window");
            UnregisterClassA(self.class_name, Some(self.h_instance))
                .expect("failed to unregister class");
        }
    }
}
impl KbmCapture {
    pub fn initialize() -> Result<Self> {
        unsafe {
            let class_name = PCSTR(c"RawInputWindowClass".to_bytes_with_nul().as_ptr());
            let h_instance: HINSTANCE = GetModuleHandleA(None)?.into();

            let wc = WNDCLASSA {
                lpfnWndProc: Some(Self::window_proc),
                hInstance: h_instance,
                lpszClassName: class_name,
                ..Default::default()
            };

            if RegisterClassA(&wc) == 0 {
                use windows::Win32::Foundation::GetLastError;
                let error = GetLastError();
                bail!("failed to register window class: {error:?}");
            }

            let hwnd = CreateWindowExA(
                WINDOW_EX_STYLE(0),
                class_name,
                PCSTR::null(),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(h_instance),
                None,
            )
            .wrap_err("failed to create window")?;

            tracing::debug!("RawInput window created: {hwnd:?}");

            let raw_input_devices = [
                0x02, // Mouse
                0x06, // Keyboard
            ]
            .map(|usage| RAWINPUTDEVICE {
                usUsagePage: 0x01, // Generic Desktop Controls
                usUsage: usage,
                dwFlags: RIDEV_INPUTSINK, // Receive input even when not in foreground
                hwndTarget: hwnd,
            });

            RegisterRawInputDevices(
                &raw_input_devices,
                size_of::<RAWINPUTDEVICE>()
                    .try_into()
                    .expect("size of RAWINPUTDEVICE should fit in u32"),
            )
            .wrap_err("failed to register raw input devices")?;

            Ok(Self {
                hwnd,
                class_name,
                h_instance,
            })
        }
    }

    pub fn run_queue(mut event_callback: impl FnMut(Event)) -> Result<()> {
        unsafe {
            let mut msg = MSG::default();
            let mut last_absolute: Option<(i32, i32)> = None;
            while GetMessageA(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageA(&msg);
                if msg.message == WindowsAndMessaging::WM_INPUT {
                    for event in parse_wm_input(msg.lParam, &mut last_absolute) {
                        event_callback(event);
                    }
                }
            }
            Ok(())
        }
    }

    #[tracing::instrument(skip_all, fields(hwnd = ?hwnd))]
    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging;
            match msg {
                WindowsAndMessaging::WM_CREATE => {
                    tracing::debug!(msg = "WM_CREATE");
                    LRESULT(0)
                }
                WindowsAndMessaging::WM_DESTROY => {
                    tracing::debug!(msg = "WM_DESTROY");
                    PostQuitMessage(0);
                    LRESULT(0)
                }

                _ => DefWindowProcA(hwnd, msg, wparam, lparam),
            }
        }
    }
}

fn parse_wm_input(lparam: LPARAM, last_absolute: &mut Option<(i32, i32)>) -> Vec<Event> {
    unsafe {
        let hrawinput = HRAWINPUT(std::ptr::with_exposed_provenance_mut(lparam.0 as usize));
        let mut rawinput = RAWINPUT::default();
        let mut pcbsize = size_of_val(&rawinput) as u32;
        let result = GetRawInputData(
            hrawinput,
            RID_INPUT,
            Some(&mut rawinput as *mut _ as *mut _),
            &mut pcbsize,
            size_of::<RAWINPUTHEADER>()
                .try_into()
                .expect("size of HRAWINPUT should fit in u32"),
        );
        if result == u32::MAX {
            return Vec::new();
        }

        match Input::RID_DEVICE_INFO_TYPE(rawinput.header.dwType) {
            Input::RIM_TYPEMOUSE => {
                let mut events = Vec::new();

                let mouse = rawinput.data.mouse;
                let us_flags = mouse.usFlags.0;

                // Handle mouse movement
                if mouse.lLastX != 0 || mouse.lLastY != 0 {
                    let (delta_x, delta_y) = if (us_flags & MOUSE_MOVE_ABSOLUTE.0) != 0 {
                        // Absolute movement - convert to screen coordinates and calculate delta
                        let is_virtual_desktop = (us_flags & MOUSE_VIRTUAL_DESKTOP.0) != 0;
                        let (screen_x, screen_y) = convert_absolute_to_screen_coords(
                            mouse.lLastX,
                            mouse.lLastY,
                            is_virtual_desktop,
                        );

                        let delta = last_absolute
                            .map(|(last_x, last_y)| (screen_x - last_x, screen_y - last_y))
                            .unwrap_or_default();

                        // Update stored absolute position
                        *last_absolute = Some((screen_x, screen_y));

                        delta
                    } else {
                        // Relative movement - use raw values directly
                        (mouse.lLastX, mouse.lLastY)
                    };

                    if delta_x != 0 || delta_y != 0 {
                        events.push(Event::MouseMove([delta_x, delta_y]));
                    }
                }

                let us_button_flags = u32::from(mouse.Anonymous.Anonymous.usButtonFlags);

                if us_button_flags & RI_MOUSE_LEFT_BUTTON_DOWN != 0 {
                    events.push(Event::MousePress {
                        key: VK_LBUTTON.0,
                        press_state: PressState::Pressed,
                    });
                }
                if us_button_flags & RI_MOUSE_LEFT_BUTTON_UP != 0 {
                    events.push(Event::MousePress {
                        key: VK_LBUTTON.0,
                        press_state: PressState::Released,
                    });
                }
                if us_button_flags & RI_MOUSE_RIGHT_BUTTON_DOWN != 0 {
                    events.push(Event::MousePress {
                        key: VK_RBUTTON.0,
                        press_state: PressState::Pressed,
                    });
                }
                if us_button_flags & RI_MOUSE_RIGHT_BUTTON_UP != 0 {
                    events.push(Event::MousePress {
                        key: VK_RBUTTON.0,
                        press_state: PressState::Released,
                    });
                }
                if us_button_flags & RI_MOUSE_MIDDLE_BUTTON_DOWN != 0 {
                    events.push(Event::MousePress {
                        key: VK_MBUTTON.0,
                        press_state: PressState::Pressed,
                    });
                }
                if us_button_flags & RI_MOUSE_MIDDLE_BUTTON_UP != 0 {
                    events.push(Event::MousePress {
                        key: VK_MBUTTON.0,
                        press_state: PressState::Released,
                    });
                }
                if us_button_flags & RI_MOUSE_BUTTON_4_DOWN != 0 {
                    events.push(Event::MousePress {
                        key: VK_XBUTTON1.0,
                        press_state: PressState::Pressed,
                    });
                }
                if us_button_flags & RI_MOUSE_BUTTON_4_UP != 0 {
                    events.push(Event::MousePress {
                        key: VK_XBUTTON1.0,
                        press_state: PressState::Released,
                    });
                }
                if us_button_flags & RI_MOUSE_BUTTON_5_DOWN != 0 {
                    events.push(Event::MousePress {
                        key: VK_XBUTTON2.0,
                        press_state: PressState::Pressed,
                    });
                }
                if us_button_flags & RI_MOUSE_BUTTON_5_UP != 0 {
                    events.push(Event::MousePress {
                        key: VK_XBUTTON2.0,
                        press_state: PressState::Released,
                    });
                }

                if us_button_flags & RI_MOUSE_WHEEL != 0 {
                    events.push(Event::MouseScroll {
                        scroll_amount: mouse.Anonymous.Anonymous.usButtonData as i16,
                    });
                }

                events
            }
            Input::RIM_TYPEKEYBOARD => {
                let keyboard = rawinput.data.keyboard;
                let key = keyboard.VKey;
                let flags = u32::from(keyboard.Flags);
                let press_state = if flags & RI_KEY_BREAK != 0 {
                    PressState::Released
                } else {
                    PressState::Pressed
                };
                vec![Event::KeyPress { key, press_state }]
            }
            _ => vec![],
        }
    }
}

/// Convert normalized absolute mouse coordinates to screen coordinates
/// Based on Microsoft documentation: coordinates are normalized between 0 and 65535
/// Accounts for virtual desktop if the MOUSE_VIRTUAL_DESKTOP flag is set
fn convert_absolute_to_screen_coords(x: i32, y: i32, is_virtual_desktop: bool) -> (i32, i32) {
    let (left, top, right, bottom) = unsafe {
        if is_virtual_desktop {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        } else {
            (
                0,
                0,
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
            )
        }
    };

    // Convert from normalized coordinates (0-65535) to screen coordinates
    // Using MulDiv equivalent: (x * (right - left)) / 65535 + left
    let screen_x = (x * (right - left)) / 65535 + left;
    let screen_y = (y * (bottom - top)) / 65535 + top;

    (screen_x, screen_y)
}
