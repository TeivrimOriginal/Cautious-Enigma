//! Системные горячие клавиши: работают, когда окно приложения свёрнуто.
//!
//! Клавиши регистрируются в Windows через `RegisterHotKey`, поэтому срабатывают
//! независимо от активного окна. События читаются в отдельном потоке и
//! отправляются в цикл событий окна через прокси.

use std::thread;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tao::event_loop::EventLoopProxy;

use crate::AppEvent;

/// Описание горячих клавиш: показывается в справке и в палитре команд.
pub const BINDINGS: [(&str, &str); 3] = [
    ("Ctrl+Alt+L", "открыть окно и начать повторение"),
    ("Ctrl+Alt+S", "сохранить резервную копию прогресса"),
    ("Ctrl+Alt+P", "свернуть или восстановить окно"),
];

/// Держит регистрацию горячих клавиш: при выгрузке они снимаются.
pub struct Hotkeys {
    _manager: GlobalHotKeyManager,
}

/// Собирает список «клавиши → действие».
fn bindings() -> Vec<(HotKey, AppEvent)> {
    let modifiers = Modifiers::CONTROL | Modifiers::ALT;
    vec![
        (
            HotKey::new(Some(modifiers), Code::KeyL),
            AppEvent::ShowReview,
        ),
        (
            HotKey::new(Some(modifiers), Code::KeyS),
            AppEvent::SaveBackup,
        ),
        (
            HotKey::new(Some(modifiers), Code::KeyP),
            AppEvent::ToggleWindow,
        ),
    ]
}

/// Регистрирует горячие клавиши и запускает поток обработки нажатий.
pub fn register(events: EventLoopProxy<AppEvent>) -> Option<Hotkeys> {
    let manager = GlobalHotKeyManager::new().ok()?;
    let bindings = bindings();

    for (hotkey, _) in &bindings {
        match manager.register(*hotkey) {
            Ok(()) => println!("горячая клавиша {} занята оболочкой", describe(hotkey)),
            // Занятая чужой программой клавиша не должна мешать запуску.
            Err(err) => println!("горячая клавиша {} недоступна: {err}", describe(hotkey)),
        }
    }

    let ids: Vec<(u32, String, AppEvent)> = bindings
        .iter()
        .map(|(hotkey, action)| (hotkey.id(), describe(hotkey), action.clone()))
        .collect();

    thread::spawn(move || {
        // Канал живёт всё время процесса, поэтому блокирующий приём безопасен.
        while let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
            if event.state() != HotKeyState::Pressed {
                continue;
            }
            let Some((_, keys, action)) = ids.iter().find(|(id, _, _)| *id == event.id()) else {
                continue;
            };
            println!("горячая клавиша {keys} сработала");
            if events.send_event(action.clone()).is_err() {
                break;
            }
        }
    });

    Some(Hotkeys { _manager: manager })
}

/// Читаемое имя клавиши для сообщений об ошибках.
fn describe(hotkey: &HotKey) -> String {
    let name = hotkey.key.to_string();
    let key = name.strip_prefix("Key").unwrap_or(&name);
    format!("Ctrl+Alt+{key}")
        .to_uppercase()
        .replace("CTRL", "Ctrl")
        .replace("ALT", "Alt")
}

/// Описание горячих клавиш для JSON-ответа `info`.
pub fn described() -> Vec<serde_json::Value> {
    BINDINGS
        .iter()
        .map(|(keys, action)| serde_json::json!({ "keys": keys, "action": action }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_unique() {
        let mut ids: Vec<u32> = bindings().iter().map(|(hotkey, _)| hotkey.id()).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            total,
            "идентификаторы горячих клавиш должны различаться"
        );
    }

    #[test]
    fn prints_identifiers_for_manual_checks() {
        for (hotkey, _) in bindings() {
            println!(
                "{} → id={} key={:?}",
                describe(&hotkey),
                hotkey.id(),
                hotkey.key
            );
        }
    }
}
