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
                RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK,
                RegisterRawInputDevices,
            },
            WindowsAndMessaging::{
                self, CreateWindowExA, DefWindowProcA, DestroyWindow, DispatchMessageA,
                GetMessageA, HWND_MESSAGE, MSG, PostQuitMessage, RI_KEY_BREAK,
                RI_MOUSE_BUTTON_4_DOWN, RI_MOUSE_BUTTON_4_UP, RI_MOUSE_BUTTON_5_DOWN,
                RI_MOUSE_BUTTON_5_UP, RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_LEFT_BUTTON_UP,
                RI_MOUSE_MIDDLE_BUTTON_DOWN, RI_MOUSE_MIDDLE_BUTTON_UP, RI_MOUSE_RIGHT_BUTTON_DOWN,
                RI_MOUSE_RIGHT_BUTTON_UP, RI_MOUSE_WHEEL, RegisterClassA, TranslateMessage,
                UnregisterClassA, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSA,
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
            while GetMessageA(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageA(&msg);
                if msg.message == WindowsAndMessaging::WM_INPUT {
                    for event in parse_wm_input(msg.lParam) {
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

fn parse_wm_input(lparam: LPARAM) -> Vec<Event> {
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
                if mouse.lLastX != 0 || mouse.lLastY != 0 {
                    events.push(Event::MouseMove([mouse.lLastX, mouse.lLastY]));
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
