//! Десктопная оболочка LinguaRust: нативное окно WebView2 и локальный сервер.
//!
//! Приложение не требует Node.js, Python или внешнего сервера: при старте
//! поднимается случайный порт на `127.0.0.1`, а в окно загружается тот же
//! офлайн-сайт, который используется в GitHub Pages. Запросы к `/__app/...`
//! подписаны токеном запуска и дают вебу нативные возможности: резервные
//! копии прогресса на диск, экспорт, каталог данных, автозапуск и тема
//! заголовка окна.

mod appdata;
mod autostart;
mod cli;
mod server;
mod util;

use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
use tao::window::{Theme, Window, WindowBuilder};
use wry::WebViewBuilder;

use appdata::{AppData, WindowState};
use cli::Launch;

/// Минимальный размер окна.
const MIN_WIDTH: f64 = 900.0;
const MIN_HEIGHT: f64 = 600.0;
/// Размер окна при первом запуске.
const DEFAULT_WIDTH: f64 = 1200.0;
const DEFAULT_HEIGHT: f64 = 820.0;

/// События от локального сервера в главный поток окна.
pub enum AppEvent {
    /// Сменить тему заголовка окна.
    SetTheme(String),
    /// Автозапуск изменился из интерфейса.
    AutostartChanged(bool),
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = match cli::parse(std::env::args().skip(1))? {
        Launch::Help => {
            println!("{}", cli::HELP);
            return Ok(());
        }
        Launch::Version => {
            println!("linguarust-desktop {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Launch::Run(options) => options,
    };

    let data = AppData::new(options.data_dir)?;
    let mut settings = data.load_settings();
    settings.autostart = autostart::is_enabled();

    let site = site_root()?;
    let token = launch_token();
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    // Сервер живёт в отдельных потоках и общается с окном через прокси цикла
    // событий: так решения интерфейса применяются в UI-потоке.
    let bridge = Arc::new(server::Bridge {
        data: data.clone(),
        events: event_loop.create_proxy(),
        token: token.clone(),
    });
    let port = server::start(site, bridge)?;

    let saved = settings.window;
    let window = WindowBuilder::new()
        .with_title("LinguaRust")
        .with_inner_size(LogicalSize::new(width_of(saved), height_of(saved)))
        .with_min_inner_size(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT))
        .with_resizable(true)
        .build(&event_loop)?;

    // Сохранённую позицию применяем только если окно попадает на монитор:
    // иначе пользователь мог отключить тот экран, на котором оставил окно.
    if let Some(position) = restore_position(&event_loop, saved) {
        window.set_outer_position(position);
    }
    if saved.maximized {
        window.set_maximized(true);
    }
    if options.autostart {
        window.set_minimized(true);
    }
    apply_theme(&window, &settings.theme);

    let url = format!("http://127.0.0.1:{port}/?t={token}");
    println!("LinguaRust desktop: {url}");
    println!("Данные приложения: {}", data.dir().display());

    let _webview = WebViewBuilder::new()
        .with_url(&url)
        .with_devtools(true)
        .with_initialization_script(bridge_script(&token))
        .build(&window)?;

    let mut last_save = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Решения веб-интерфейса применяем в главном потоке окна.
        if let Event::UserEvent(AppEvent::SetTheme(theme)) = &event {
            settings.theme = theme.clone();
            apply_theme(&window, theme);
            let _ = data.save_settings(&settings);
        }
        if let Event::UserEvent(AppEvent::AutostartChanged(enabled)) = &event {
            settings.autostart = *enabled;
            let _ = data.save_settings(&settings);
        }

        let Event::WindowEvent {
            event: window_event,
            ..
        } = &event
        else {
            return;
        };
        match window_event {
            // Геометрия запоминается, но не чаще раза в пару секунд, чтобы
            // при перетаскивании окна не писать файл на каждый кадр.
            WindowEvent::Resized(_) | WindowEvent::Moved(_) => {
                settings.window = capture_geometry(&window, settings.window);
                if last_save.elapsed() >= Duration::from_secs(2) {
                    let _ = data.save_settings(&settings);
                    last_save = Instant::now();
                }
            }
            WindowEvent::CloseRequested => {
                settings.window = capture_geometry(&window, settings.window);
                let _ = data.save_settings(&settings);
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

fn width_of(state: WindowState) -> f64 {
    if state.width == 0 {
        DEFAULT_WIDTH
    } else {
        f64::from(state.width.max(MIN_WIDTH as u32))
    }
}

fn height_of(state: WindowState) -> f64 {
    if state.height == 0 {
        DEFAULT_HEIGHT
    } else {
        f64::from(state.height.max(MIN_HEIGHT as u32))
    }
}

/// Возвращает сохранённую позицию, если окно остаётся видимым на мониторе.
fn restore_position<T: 'static>(
    event_loop: &EventLoop<T>,
    saved: WindowState,
) -> Option<PhysicalPosition<i32>> {
    if saved.x == 0 && saved.y == 0 {
        return None;
    }

    let left = i64::from(saved.x);
    let top = i64::from(saved.y);
    let right = left + i64::from(saved.width);
    let bottom = top + i64::from(saved.height);

    let visible = event_loop.available_monitors().any(|monitor| {
        let origin = monitor.position();
        let size = monitor.size();
        let monitor_left = i64::from(origin.x);
        let monitor_top = i64::from(origin.y);
        let monitor_right = monitor_left + i64::from(size.width);
        let monitor_bottom = monitor_top + i64::from(size.height);
        let overlap_x = (right.min(monitor_right) - left.max(monitor_left)).max(0);
        let overlap_y = (bottom.min(monitor_bottom) - top.max(monitor_top)).max(0);
        // Заголовок и кнопки управления должны быть доступны пользователю.
        overlap_x >= 160 && overlap_y >= 80
    });

    visible.then(|| PhysicalPosition::new(saved.x, saved.y))
}

/// Запоминает размеры и позицию окна, пока оно не свёрнуто и не развёрнуто.
fn capture_geometry(window: &Window, previous: WindowState) -> WindowState {
    let maximized = window.is_maximized();
    if maximized || window.is_minimized() {
        return WindowState {
            maximized,
            ..previous
        };
    }
    let size = window.inner_size();
    let position = window
        .outer_position()
        .unwrap_or_else(|_| PhysicalPosition::new(previous.x, previous.y));
    WindowState {
        width: size.width,
        height: size.height,
        x: position.x,
        y: position.y,
        maximized,
    }
}

/// Тёмный заголовок окна повторяет тему приложения (Windows 11).
fn apply_theme(window: &Window, theme: &str) {
    window.set_theme(Some(if theme == "dark" {
        Theme::Dark
    } else {
        Theme::Light
    }));
}

/// Подставляет токен запуска в скрипт моста.
fn bridge_script(token: &str) -> String {
    include_str!("bridge.js").replace("__LINGUARUST_TOKEN__", token)
}

/// Случайный токен запуска: без него любой локальный процесс мог бы писать
/// в настройки и файлы прогресса через случайный порт.
fn launch_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let first = {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(u64::from(std::process::id()));
        hasher.write_u64(util::unix_seconds());
        hasher.finish()
    };
    let second = {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(first);
        hasher.write_u64(util::unix_seconds());
        hasher.finish()
    };
    format!("{first:016x}{second:016x}")
}

/// Ищет каталог с `index.html`: сначала собранный `site`, затем исходники.
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
            return Ok(std::fs::canonicalize(candidate)?);
        }
    }

    Err("не найден desktop/site/index.html (запусти sync-site.ps1)".into())
}
