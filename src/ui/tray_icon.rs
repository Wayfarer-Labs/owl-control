use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use color_eyre::eyre::{self, Context as _};
use tray_icon::{
    MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuId, MenuItem},
};
use winit::window::Window;

use crate::{
    app_state::{UiUpdate, UiUpdateSender},
    assets,
};

pub struct TrayIconState {
    icon: TrayIcon,
    quit_item_id: MenuId,

    default_tray_icon_data: tray_icon::Icon,
    recording_tray_icon_data: tray_icon::Icon,
}
impl TrayIconState {
    pub fn new() -> eyre::Result<Self> {
        // tray icon right click menu for quit option
        let quit_item = MenuItem::new("Quit", true, None);
        let quit_item_id = quit_item.id().clone();
        let tray_menu = Menu::new();
        let _ = tray_menu.append(&quit_item);

        // create tray icon
        fn create_tray_icon_data_from_bytes(bytes: &[u8]) -> eyre::Result<tray_icon::Icon> {
            let (rgba, (width, height)) = assets::load_icon_data_from_bytes(bytes);
            Ok(tray_icon::Icon::from_rgba(rgba, width, height)?)
        }
        let default_tray_icon_data =
            create_tray_icon_data_from_bytes(assets::get_logo_default_bytes())
                .context("Failed to create default tray icon")?;
        let recording_tray_icon_data =
            create_tray_icon_data_from_bytes(assets::get_logo_recording_bytes())
                .context("Failed to create recording tray icon")?;

        let tray_icon = TrayIconBuilder::new()
            .with_icon(default_tray_icon_data.clone())
            .with_tooltip("OWL Control")
            .with_menu(Box::new(tray_menu))
            .build()?;

        Ok(TrayIconState {
            icon: tray_icon,
            quit_item_id,
            default_tray_icon_data,
            recording_tray_icon_data,
        })
    }

    /// Called once the egui context is available
    pub fn post_initialize(
        &self,
        context: egui::Context,
        window: Arc<Window>,
        visible: Arc<AtomicBool>,
        stopped_tx: tokio::sync::broadcast::Sender<()>,
        ui_update_tx: UiUpdateSender,
    ) {
        MenuEvent::set_event_handler({
            let quit_item_id = self.quit_item_id.clone();
            let window = window.clone();
            let visible = visible.clone();
            Some(move |event: MenuEvent| match event.id() {
                id if id == &quit_item_id => {
                    tracing::info!("Tray icon requested shutdown");
                    stopped_tx.send(()).unwrap();

                    // a bit hacky, but we have to force the window to be visible again so that the MainApp can call main loop repaint,
                    // otherwise the app will remain active until the user clicks on tray icon to reopen it, and then it will kill itself
                    if !visible.load(Ordering::Relaxed) {
                        window.set_visible(true);
                        // window.focus_window();
                        visible.store(true, Ordering::Relaxed);
                    }
                    // have to unminimize and focus the window to ensure that redraw and subsequent recv() of stop is called in App
                    window.set_minimized(false);
                    window.focus_window();

                    ui_update_tx.blocking_send(UiUpdate::ForceUpdate).ok();
                }
                _ => {}
            })
        });

        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: MouseButtonState::Down,
                ..
            } = event
            {
                if visible.load(Ordering::Relaxed) {
                    window.set_visible(false);
                    visible.store(false, Ordering::Relaxed);
                } else {
                    // set viewport visible true in case it was minimised to tray via closing the app
                    window.set_visible(true);
                    visible.store(true, Ordering::Relaxed);
                }
                context.request_repaint();
            }
        }));
    }

    pub fn set_icon_recording(&self, recording: bool) {
        self.icon
            .set_icon(Some(if recording {
                self.recording_tray_icon_data.clone()
            } else {
                self.default_tray_icon_data.clone()
            }))
            .ok();
    }
}
