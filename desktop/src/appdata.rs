//! Файлы приложения: настройки окна и резервные копии прогресса.
//!
//! По умолчанию всё лежит в `%APPDATA%\LinguaRust`, поэтому данные не зависят
//! от того, из какого каталога запущен бинарник, и переживают его пересборку.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::util;

/// Настройки приложения (сериализуются в `settings.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub window: WindowState,
    /// Тема заголовка окна: `dark` или `light`.
    pub theme: String,
    /// Предыдущее состояние автозапуска (для информации в интерфейсе).
    pub autostart: bool,
}

/// Геометрия окна между запусками.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1200,
            height: 820,
            x: 0,
            y: 0,
            maximized: false,
        }
    }
}

/// Резервная копия прогресса (сериализуется в `progress.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub saved_at: String,
    pub payload: String,
}

/// Каталог с данными приложения.
#[derive(Debug, Clone)]
pub struct AppData {
    dir: PathBuf,
}

impl AppData {
    /// Создаёт каталог данных: явно заданный или `%APPDATA%\LinguaRust`.
    pub fn new(explicit: Option<PathBuf>) -> io::Result<Self> {
        let dir = match explicit {
            Some(path) => path,
            None => {
                let base = std::env::var_os("APPDATA")
                    .map(PathBuf::from)
                    .or_else(|| {
                        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                    })
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, "не найден каталог APPDATA")
                    })?;
                base.join("LinguaRust")
            }
        };
        fs::create_dir_all(dir.join("exports"))?;
        Ok(Self { dir })
    }

    /// Каталог данных.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Читает настройки, при любой ошибке возвращает значения по умолчанию.
    pub fn load_settings(&self) -> Settings {
        fs::read_to_string(self.dir.join("settings.json"))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Сохраняет настройки.
    pub fn save_settings(&self, settings: &Settings) -> io::Result<()> {
        let body = serde_json::to_string_pretty(settings)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        write_atomic(&self.dir.join("settings.json"), &body)
    }

    /// Записывает последнюю резервную копию прогресса.
    pub fn save_backup(&self, payload: &str) -> io::Result<Backup> {
        let backup = Backup {
            saved_at: util::now_iso(),
            payload: payload.to_string(),
        };
        let body = serde_json::to_string(&backup)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        write_atomic(&self.dir.join("progress.json"), &body)?;
        Ok(backup)
    }

    /// Читает последнюю резервную копию, если она есть.
    pub fn load_backup(&self) -> Option<Backup> {
        let raw = fs::read_to_string(self.dir.join("progress.json")).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Сохраняет отдельный файл экспорта с датой в имени.
    pub fn export(&self, payload: &str) -> io::Result<PathBuf> {
        let path = self
            .dir
            .join("exports")
            .join(format!("linguarust-{}.json", util::now_stamp()));
        write_atomic(&path, payload)?;
        Ok(path)
    }

    /// Открывает каталог данных в Проводнике.
    pub fn open_dir(&self) -> io::Result<()> {
        util::spawn_detached("explorer.exe", &[self.dir.as_os_str()])
    }
}

/// Пишет файл через временный соседний, чтобы не оставлять обрезанные данные.
fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, contents)?;
    fs::rename(&temp, path)
}
