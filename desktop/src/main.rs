//! Десктопная оболочка LinguaRust: нативное окно WebView2 и локальный
//! HTTP-сервер для офлайн-ресурсов из `desktop/site`.
//!
//! Приложение не требует Node.js, Python или внешнего сервера: при старте
//! поднимается случайный порт на `127.0.0.1`, а в окно загружается тот же
//! офлайн-сайт, который используется в GitHub Pages.

use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

fn main() -> Result<(), Box<dyn Error>> {
    let site = site_root()?;
    let port = start_server(site)?;
    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("LinguaRust")
        .with_inner_size(LogicalSize::new(1200.0, 820.0))
        .with_min_inner_size(LogicalSize::new(900.0, 600.0))
        .with_resizable(true)
        .build(&event_loop)?;

    let url = format!("http://127.0.0.1:{port}/");
    let _webview = WebViewBuilder::new()
        .with_url(&url)
        .with_devtools(true)
        .build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

/// Ищет каталог с `index.html`: сначала рядом с exe, затем в исходном проекте.
fn site_root() -> Result<PathBuf, Box<dyn Error>> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut candidates = Vec::new();

    // Собранный `desktop/site` ищем первым: только он содержит `data/` и
    // `style.css`, которые ждёт офлайн-версия сайта.
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("site"));
        candidates.push(dir.join("..").join("site"));
        candidates.push(dir.join("..").join("..").join("site"));
    }
    candidates.push(manifest.join("site"));
    if let Ok(current) = std::env::current_dir() {
        candidates.push(current.join("desktop").join("site"));
        candidates.push(current.join("site"));
    }
    if let Some(parent) = manifest.parent() {
        candidates.push(parent.join("web"));
    }

    for candidate in candidates {
        if candidate.join("index.html").is_file() {
            return Ok(fs::canonicalize(candidate)?);
        }
    }

    Err("не найден desktop/site/index.html (запусти sync-site.ps1)".into())
}

/// Поднимает локальный сервер на случайном свободном порту.
fn start_server(root: PathBuf) -> Result<u16, Box<dyn Error>> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();

    thread::spawn(move || {
        for incoming in listener.incoming() {
            let Ok(stream) = incoming else { continue };
            let root = root.clone();
            thread::spawn(move || {
                let _ = serve_client(stream, &root);
            });
        }
    });

    Ok(port)
}

fn serve_client(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut buffer = [0_u8; 8192];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let target = request
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/");

    let relative = if target == "/" {
        "index.html"
    } else {
        target.trim_start_matches('/')
    };

    // Не выпускаем запрос за пределы каталога сайта.
    if relative.split('/').any(|part| part == "..") {
        return write_response(&mut stream, 404, "text/plain; charset=utf-8", b"Not Found");
    }

    let file = root.join(relative);
    if !file.is_file() {
        return write_response(&mut stream, 404, "text/plain; charset=utf-8", b"Not Found");
    }
    let body = fs::read(file)?;
    let content_type = content_type(relative);
    write_response(&mut stream, 200, content_type, &body)
}

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

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-cache\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}
