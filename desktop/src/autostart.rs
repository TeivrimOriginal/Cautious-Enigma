//! Автозапуск вместе с Windows: ветка реестра `HKCU\...\Run`.
//!
//! Ключ создаётся для текущего пользователя, поэтому права администратора
//! не нужны, а запись удаляется вместе с приложением.

use std::io;
use std::path::Path;

/// Раздел реестра автозапуска.
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
/// Имя значения в ветке автозапуска.
const VALUE_NAME: &str = "LinguaRust";
/// CREATE_NO_WINDOW: `reg.exe` не должен мигать консолью.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Включён ли автозапуск.
pub fn is_enabled() -> bool {
    #[cfg(windows)]
    {
        reg(&["query", RUN_KEY, "/v", VALUE_NAME])
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Включает или выключает автозапуск текущего бинарника.
///
/// При включении в значение добавляется аргумент `--autostart`, чтобы окно
/// открывалось свёрнутым и не мешало работе.
pub fn set_enabled(enabled: bool, exe: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        if enabled {
            let command = format!("\"{}\" --autostart", exe.display());
            let output = reg(&[
                "add", RUN_KEY, "/v", VALUE_NAME, "/t", "REG_SZ", "/d", &command, "/f",
            ])?;
            if output.status.success() {
                Ok(())
            } else {
                Err(io::Error::other(
                    String::from_utf8_lossy(&output.stderr).trim().to_string(),
                ))
            }
        } else {
            // Ключа может не быть — тогда автозапуск уже выключен.
            let _ = reg(&["delete", RUN_KEY, "/v", VALUE_NAME, "/f"]);
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (enabled, exe);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "автозапуск поддерживается только в Windows",
        ))
    }
}

/// Запускает `reg.exe` и возвращает вывод команды.
#[cfg(windows)]
fn reg(args: &[&str]) -> io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    Command::new("reg.exe")
        .args(args)
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
}
