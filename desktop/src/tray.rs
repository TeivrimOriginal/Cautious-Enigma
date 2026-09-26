//! Значок в системном трее: меню, показ окна и свертывание вместо закрытия.

use std::thread;
use std::time::Duration;

use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tao::event_loop::EventLoopProxy;
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::AppEvent;
use crate::icon;

/// Идентификаторы пунктов меню трея.
mod id {
    pub const OPEN: &str = "open";
    pub const REVIEW: &str = "review";
    pub const BACKUP: &str = "backup";
    pub const CLOSE_TO_TRAY: &str = "close-to-tray";
    pub const QUIT: &str = "quit";
}

/// Значок в трее. При выгрузке значок исчезает, а окно закрывается.
pub struct Tray {
    icon: TrayIcon,
    close_to_tray: CheckMenuItem,
}

impl Tray {
    /// Создаёт значок и поток обработки кликов и меню.
    pub fn new(proxy: &EventLoopProxy<AppEvent>, close_to_tray: bool) -> Result<Self, String> {
        let menu = Menu::new();
        let open = MenuItem::with_id(id::OPEN, "Открыть LinguaRust", true, None);
        let review = MenuItem::with_id(id::REVIEW, "Начать повторение", true, None);
        let backup = MenuItem::with_id(id::BACKUP, "Сохранить резервную копию", true, None);
        let close = CheckMenuItem::with_id(
            id::CLOSE_TO_TRAY,
            "Закрывать в трей вместо выхода",
            true,
            close_to_tray,
            None,
        );
        let quit = MenuItem::with_id(id::QUIT, "Выход", true, None);

        menu.append(&open).map_err(|err| err.to_string())?;
        menu.append(&review).map_err(|err| err.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|err| err.to_string())?;
        menu.append(&backup).map_err(|err| err.to_string())?;
        menu.append(&close).map_err(|err| err.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|err| err.to_string())?;
        menu.append(&quit).map_err(|err| err.to_string())?;

        let icon_image = Icon::from_rgba(icon::tray_rgba(), icon::TRAY_SIZE, icon::TRAY_SIZE)
            .map_err(|err| format!("иконка трея: {err}"))?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("LinguaRust")
            .with_icon(icon_image)
            .build()
            .map_err(|err| format!("значок в трее: {err}"))?;

        spawn_event_thread(proxy.clone());
        Ok(Self {
            icon: tray,
            close_to_tray: close,
        })
    }

    /// Обновляет подпись значка: сколько карточек ждёт повторения.
    pub fn set_status(&self, due: usize, cards: usize) {
        let tooltip = match (due, cards) {
            (0, 0) => "LinguaRust · начните обучение".to_string(),
            (0, cards) => format!("LinguaRust · {cards} карточек в работе"),
            (due, _) => format!("LinguaRust · {due} к повторению"),
        };
        if let Err(err) = self.icon.set_tooltip(Some(tooltip)) {
            eprintln!("не удалось обновить подпись трея: {err}");
        }
    }

    /// Показывает или скрывает галочку «Закрывать в трей».
    pub fn set_close_to_tray(&self, enabled: bool) {
        self.close_to_tray.set_checked(enabled);
    }
}

/// Поток событий: клик по значку открывает окно, меню отправляет действия.
fn spawn_event_thread(proxy: EventLoopProxy<AppEvent>) {
    thread::spawn(move || {
        loop {
            // Значок опрашиваем чаще меню: у него есть собственный канал событий.
            if let Ok(event) = TrayIconEvent::receiver().recv_timeout(Duration::from_millis(150)) {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let _ = proxy.send_event(AppEvent::ShowWindow);
                }
                continue;
            }
            if let Ok(event) = MenuEvent::receiver().recv_timeout(Duration::from_millis(50)) {
                let action = match event.id.as_ref() {
                    id::OPEN => AppEvent::ShowWindow,
                    id::REVIEW => AppEvent::ShowReview,
                    id::BACKUP => AppEvent::SaveBackup,
                    id::CLOSE_TO_TRAY => AppEvent::ToggleCloseToTray,
                    id::QUIT => AppEvent::Quit,
                    _ => continue,
                };
                if proxy.send_event(action).is_err() {
                    break;
                }
            }
        }
    });
}
