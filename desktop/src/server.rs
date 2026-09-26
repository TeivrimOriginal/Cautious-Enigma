//! Локальный HTTP-сервер: раздача офлайн-сайта и мост `window.linguarustDesktop`.
//!
//! Сервер поднимается на случайном свободном порту `127.0.0.1` и обслуживает
//! два вида запросов:
//!
//! * `/...` — статические файлы из каталога сайта (нужно, чтобы работали
//!   `fetch` к данным, `localStorage` и service worker: из `file://` они
//!   заблокированы политиками Chromium);
//! * `/__app/...` — нативные возможности оболочки: резервные копии прогресса,
//!   экспорт, каталог данных, автозапуск и тема окна.

use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tao::event_loop::EventLoopProxy;

use crate::AppEvent;
use crate::appdata::AppData;
use crate::autostart;

/// Предел для заголовков запроса.
const MAX_HEADER: usize = 16 * 1024;
/// Предел для тела запроса: прогресс небольшой, но импорт бывает большим.
const MAX_BODY: usize = 8 * 1024 * 1024;
/// Заголовок с токеном запуска.
const TOKEN_HEADER: &str = "x-linguarust-token";

/// Состояние нативной части приложения, доступное серверу.
pub struct Bridge {
    pub data: AppData,
    /// Прокси цикла событий окна: решения интерфейса применяются в UI-потоке.
    pub events: EventLoopProxy<AppEvent>,
    /// Токен текущего запуска для проверки запросов моста.
    pub token: String,
}

/// Запускает сервер и возвращает выбранный порт.
pub fn start(root: PathBuf, bridge: Arc<Bridge>) -> io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();

    thread::spawn(move || {
        for incoming in listener.incoming() {
            let Ok(stream) = incoming else { continue };
            let root = root.clone();
            let bridge = Arc::clone(&bridge);
            thread::spawn(move || {
                let _ = serve(stream, &root, &bridge);
            });
        }
    });

    Ok(port)
}

/// Обслуживает один запрос: сначала пробуем мост, иначе отдаём статику.
fn serve(mut stream: TcpStream, root: &Path, bridge: &Bridge) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    let (head, body) = read_request(&mut stream)?;

    let mut parts = head.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let path = target.split('?').next().unwrap_or("/").to_string();

    if let Some(action) = path.strip_prefix("/__app/") {
        if !token_matches(&head, &bridge.token) {
            let body = r#"{"ok":false,"error":"неверный токен запуска"}"#;
            return respond(&mut stream, 403, json_text(), body.as_bytes());
        }
        let (status, body) = handle_bridge(bridge, &method, action, &body);
        return respond(&mut stream, status, json_text(), body.as_bytes());
    }

    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, 405, plain_text(), b"Method Not Allowed");
    }

    let (status, content_type, body) = static_file(root, &path, method == "HEAD");
    respond(&mut stream, status, content_type, &body)
}

/// Обрабатывает действие моста и возвращает статус с JSON-ответом.
fn handle_bridge(bridge: &Bridge, method: &str, action: &str, body: &str) -> (u16, String) {
    let request = || -> Value { serde_json::from_str(body).unwrap_or(Value::Null) };

    match (method, action) {
        ("GET", "info") => {
            let data_dir = bridge.data.dir().display().to_string();
            (
                200,
                json!({
                    "ok": true,
                    "version": env!("CARGO_PKG_VERSION"),
                    "mode": "desktop",
                    "platform": std::env::consts::OS,
                    "dataDir": data_dir,
                    "autostart": autostart::is_enabled(),
                    "hotkeys": crate::hotkeys::described(),
                })
                .to_string(),
            )
        }
        ("GET", "backup") => match bridge.data.load_backup() {
            Some(backup) => (
                200,
                json!({
                    "ok": true,
                    "savedAt": backup.saved_at,
                    "payload": backup.payload,
                })
                .to_string(),
            ),
            None => (
                404,
                json!({ "ok": false, "error": "резервной копии пока нет" }).to_string(),
            ),
        },
        ("POST", "backup") => {
            let Some(payload) = payload_of(body) else {
                return (
                    400,
                    json!({ "ok": false, "error": "нет поля payload" }).to_string(),
                );
            };
            match bridge.data.save_backup(&payload) {
                Ok(backup) => (
                    200,
                    json!({
                        "ok": true,
                        "savedAt": backup.saved_at,
                        "path": bridge.data.dir().display().to_string(),
                    })
                    .to_string(),
                ),
                Err(err) => (
                    500,
                    json!({ "ok": false, "error": err.to_string() }).to_string(),
                ),
            }
        }
        ("POST", "export") => {
            let Some(payload) = payload_of(body) else {
                return (
                    400,
                    json!({ "ok": false, "error": "нет поля payload" }).to_string(),
                );
            };
            match bridge.data.export(&payload) {
                Ok(path) => (
                    200,
                    json!({ "ok": true, "path": path.display().to_string() }).to_string(),
                ),
                Err(err) => (
                    500,
                    json!({ "ok": false, "error": err.to_string() }).to_string(),
                ),
            }
        }
        ("POST", "autostart") => {
            let enabled = request()
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let exe = match std::env::current_exe() {
                Ok(path) => path,
                Err(err) => {
                    return (
                        500,
                        json!({ "ok": false, "error": err.to_string() }).to_string(),
                    );
                }
            };
            match autostart::set_enabled(enabled, &exe) {
                Ok(()) => {
                    let active = autostart::is_enabled();
                    // Главный поток обновит и сохранит настройки интерфейса.
                    let _ = bridge.events.send_event(AppEvent::AutostartChanged(active));
                    (200, json!({ "ok": true, "autostart": active }).to_string())
                }
                Err(err) => (
                    500,
                    json!({ "ok": false, "error": err.to_string() }).to_string(),
                ),
            }
        }
        ("POST", "theme") => {
            let theme = request()
                .get("theme")
                .and_then(Value::as_str)
                .unwrap_or("light")
                .to_string();
            let _ = bridge.events.send_event(AppEvent::SetTheme(theme));
            (200, json!({ "ok": true }).to_string())
        }
        ("POST", "print") => {
            // Диалог печати и «Сохранить как PDF» открывает сам WebView2.
            let _ = bridge.events.send_event(AppEvent::Print);
            (200, json!({ "ok": true }).to_string())
        }
        ("POST", "open-data-dir") => match bridge.data.open_dir() {
            Ok(()) => (
                200,
                json!({
                    "ok": true,
                    "path": bridge.data.dir().display().to_string(),
                })
                .to_string(),
            ),
            Err(err) => (
                500,
                json!({ "ok": false, "error": err.to_string() }).to_string(),
            ),
        },
        _ => (
            404,
            json!({ "ok": false, "error": "неизвестное действие" }).to_string(),
        ),
    }
}

/// Достаёт строку `payload` из JSON-тела запроса.
fn payload_of(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body)
        .ok()?
        .get("payload")?
        .as_str()
        .map(str::to_string)
}

/// Ищет файл сайта, не выпуская запрос за пределы каталога.
fn static_file(root: &Path, path: &str, head_only: bool) -> (u16, &'static str, Vec<u8>) {
    let relative = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    if relative.split('/').any(|part| part == "..") {
        return not_found();
    }

    let file = root.join(relative);
    if !file.is_file() {
        return not_found();
    }
    match fs::read(&file) {
        Ok(body) => {
            let content_type = content_type(relative);
            if head_only {
                (200, content_type, Vec::new())
            } else {
                (200, content_type, body)
            }
        }
        Err(_) => not_found(),
    }
}

fn not_found() -> (u16, &'static str, Vec<u8>) {
    (404, plain_text(), b"Not Found".to_vec())
}

/// Определяет MIME-тип по расширению файла.
fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "webmanifest" => "application/manifest+json; charset=utf-8",
        "tsv" => "text/tab-separated-values; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn json_text() -> &'static str {
    "application/json; charset=utf-8"
}

fn plain_text() -> &'static str {
    "text/plain; charset=utf-8"
}

/// Проверяет токен запуска в заголовке запроса.
fn token_matches(head: &str, token: &str) -> bool {
    head.lines()
        .filter_map(|line| line.split_once(':'))
        .any(|(name, value)| {
            name.trim().eq_ignore_ascii_case(TOKEN_HEADER) && value.trim() == token
        })
}

/// Читает заголовки и тело запроса.
fn read_request(stream: &mut TcpStream) -> io::Result<(String, String)> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0_u8; 8192];
    let head_end = loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 && buffer.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "пустой запрос",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position;
        }
        if buffer.len() > MAX_HEADER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "длинные заголовки",
            ));
        }
    };

    let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
    let length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0)
        .min(MAX_BODY);

    let body_start = (head_end + 4).min(buffer.len());
    let mut body = buffer[body_start..].to_vec();
    while body.len() < length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(length);

    Ok((head, String::from_utf8_lossy(&body).to_string()))
}

/// Отправляет ответ и закрывает соединение.
fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}
