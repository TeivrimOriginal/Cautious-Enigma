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
mod dialog;
mod hotkeys;
mod icon;
mod server;
mod tray;
mod util;
mod win32;

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
use tao::window::{Theme, UserAttentionType, Window, WindowBuilder};
use wry::{DragDropEvent, NewWindowResponse, WebViewBuilder};

use appdata::{AppData, WindowState};
use cli::Launch;

/// Минимальный размер окна.
const MIN_WIDTH: f64 = 900.0;
const MIN_HEIGHT: f64 = 600.0;
/// Размер окна при первом запуске.
const DEFAULT_WIDTH: f64 = 1200.0;
const DEFAULT_HEIGHT: f64 = 820.0;
/// Основной локальный порт: фиксированный, чтобы origin сайта не менялся.
const PORT: u16 = 47_715;
/// Предел размера файла, который можно импортировать перетаскиванием.
const MAX_DROPPED_FILE: u64 = 8 * 1024 * 1024;

/// События от локального сервера и горячих клавиш в главный поток окна.
#[derive(Clone)]
pub enum AppEvent {
    /// Сменить тему заголовка окна.
    SetTheme(String),
    /// Автозапуск изменился из интерфейса.
    AutostartChanged(bool),
    /// Открыть окно и перейти к повторению.
    ShowReview,
    /// Показать окно из трея или горячей клавиши.
    ShowWindow,
    /// Сохранить резервную копию, не дожидаясь автосохранения.
    SaveBackup,
    /// Сохранить копию по пути из нативного диалога.
    SaveCopyAs(dialog::DialogSlot, String),
    /// Открыть файл через нативный диалог.
    OpenCopy(dialog::DialogSlot),
    /// Свернуть или восстановить окно.
    ToggleWindow,
    /// Переключить режим «закрывать в трей».
    ToggleCloseToTray,
    /// Обновить подпись значка в трее.
    Status {
        due: usize,
        cards: usize,
        focus: u64,
    },
    /// Мигнуть окном в панели задач: фокус-сессия закончилась.
    Attention,
    /// Файл, перетащенный в окно.
    DroppedFile(String),
    /// Открыть диалог печати или сохранения в PDF.
    Print,
    /// Завершить приложение.
    Quit,
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
    let proxy = event_loop.create_proxy();
    let bridge = Arc::new(server::Bridge {
        data: data.clone(),
        events: proxy.clone(),
        token: token.clone(),
    });
    let port = server::start(site, bridge, PORT)?;
    if port != PORT {
        println!("порт {PORT} занят, сайт открыт на {port}: профиль будет новым");
    }
    // Системные горячие клавиши живут, пока жив менеджер.
    let _hotkeys = hotkeys::register(proxy.clone());

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
        win32::minimize(&window);
    }
    apply_theme(&window, &settings.theme);

    let url = format!("http://127.0.0.1:{port}/?t={token}");
    println!("LinguaRust desktop: {url}");
    println!("Данные приложения: {}", data.dir().display());
    for (keys, action) in hotkeys::BINDINGS {
        println!("Горячая клавиша {keys}: {action}");
    }

    // Значок в трее: окно можно свернуть вместо закрытия, а в подписи значка
    // видно, сколько карточек ждёт повторения.
    let tray = tray::Tray::new(&proxy, settings.close_to_tray);
    match &tray {
        Ok(icon) => icon.set_status(settings.due, 0, 0),
        Err(err) => println!("трей недоступен: {err}"),
    }

    // Перетаскивание файла и внешние ссылки обрабатываются нативно.
    let drop_proxy = proxy.clone();
    // Профиль WebView2 храним в каталоге данных приложения: иначе `cargo clean`
    // или перенос бинарника обнуляют localStorage сайта вместе с прогрессом.
    let user_data = data.dir().join("webview");
    fs::create_dir_all(&user_data)?;
    let mut context = wry::WebContext::new(Some(user_data));
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(&url)
        .with_devtools(true)
        .with_initialization_script(bridge_script(&token))
        .with_drag_drop_handler(move |event| {
            if let DragDropEvent::Drop { paths, .. } = event {
                for path in paths {
                    if importable_file(&path) {
                        let _ = drop_proxy
                            .send_event(AppEvent::DroppedFile(path.display().to_string()));
                        break;
                    }
                }
            }
            // Возвращаем true: файл обрабатывает оболочка, а не страница.
            true
        })
        .with_new_window_req_handler(|url, _| {
            // Внешние ссылки открываем в системном браузере, а не внутри окна.
            if url.starts_with("http://") || url.starts_with("https://") {
                open_external(&url);
            }
            NewWindowResponse::Deny
        })
        .build(&window)?;

    let mut last_save = Instant::now();
    // `context` живёт вместе с циклом событий: WebView2 теряет часть
    // возможностей, если контекст удалить раньше окна.
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
        if let Event::UserEvent(AppEvent::ShowReview) = &event {
            win32::restore(&window);
            let _ = webview.evaluate_script("location.hash = '#/'");
        }
        if let Event::UserEvent(AppEvent::ShowWindow) = &event {
            win32::restore(&window);
        }
        if let Event::UserEvent(AppEvent::SaveCopyAs(slot, payload)) = &event {
            let suggested = format!("linguarust-{}.json", util::now_stamp());
            let answer = dialog::save_copy(&suggested).and_then(|path| {
                match data.write_copy(&path, payload) {
                    Ok(()) => Some(path),
                    Err(err) => {
                        eprintln!("не удалось сохранить копию: {err}");
                        None
                    }
                }
            });
            slot.answer(answer);
        }
        if let Event::UserEvent(AppEvent::OpenCopy(slot)) = &event {
            slot.answer(dialog::open_copy());
        }
        if let Event::UserEvent(AppEvent::ToggleCloseToTray) = &event {
            settings.close_to_tray = !settings.close_to_tray;
            if let Ok(icon) = &tray {
                icon.set_close_to_tray(settings.close_to_tray);
            }
            let _ = data.save_settings(&settings);
        }
        if let Event::UserEvent(AppEvent::Status { due, cards, focus }) = &event {
            settings.due = *due;
            if let Ok(icon) = &tray {
                icon.set_status(*due, *cards, *focus);
            }
            let _ = data.save_settings(&settings);
        }
        if let Event::UserEvent(AppEvent::Attention) = &event {
            // Свёрнутое окно не видно, поэтому мигает кнопка в панели задач.
            window.request_user_attention(Some(UserAttentionType::Critical));
        }
        if let Event::UserEvent(AppEvent::Quit) = &event {
            settings.window = capture_geometry(&window, settings.window);
            let _ = data.save_settings(&settings);
            *control_flow = ControlFlow::Exit;
        }
        if let Event::UserEvent(AppEvent::ToggleWindow) = &event {
            if window.is_minimized() {
                win32::restore(&window);
            } else {
                win32::minimize(&window);
            }
        }
        if let Event::UserEvent(AppEvent::SaveBackup) = &event {
            // Копию снимает страница: только у неё есть актуальное состояние.
            let _ = webview.evaluate_script(
                "window.linguarustDesktop && window.linguarustDesktop.flushBackup()",
            );
        }
        if let Event::UserEvent(AppEvent::Print) = &event
            && let Err(err) = webview.print()
        {
            eprintln!("диалог печати не открылся: {err}");
        }
        if let Event::UserEvent(AppEvent::DroppedFile(path)) = &event {
            send_dropped_file(&webview, path);
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
                // В режиме трея закрытие окна не завершает приложение.
                if settings.close_to_tray {
                    win32::hide(&window);
                    let _ = webview.evaluate_script(
                        "window.__linguarustHidden && window.__linguarustHidden()",
                    );
                    return;
                }
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

/// Открывает ссылку в системном браузере.
fn open_external(url: &str) {
    // `explorer.exe` разбирает URL без командной строки, поэтому в ссылке
    // безопасно могут быть `&`, пробелы и кавычки.
    if let Err(err) = util::spawn_detached("explorer.exe", &[std::ffi::OsStr::new(url)]) {
        eprintln!("не удалось открыть ссылку {url}: {err}");
    }
}

/// Подходит ли файл для импорта: копия прогресса или CSV с карточками.
fn importable_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    if extension != "json" && extension != "csv" {
        return false;
    }
    fs::metadata(path)
        .map(|meta| meta.len() <= MAX_DROPPED_FILE)
        .unwrap_or(false)
}

/// Передаёт содержимое перетащенного файла в страницу как JSON-объект.
fn send_dropped_file(webview: &wry::WebView, path: &str) {
    let name = Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    match fs::read_to_string(path) {
        Ok(content) => {
            let detail = json!({ "name": name, "content": content });
            let _ = webview.evaluate_script(&format!("window.__linguarustDrop({detail})"));
        }
        Err(err) => {
            let message = json!(format!("не удалось прочитать {name}: {err}"));
            let _ = webview.evaluate_script(&format!("window.__linguarustDropError({message})"));
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Создаёт временный файл, чтобы проверить фильтр импорта.
    fn temp_file(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, b"data").expect("временный файл");
        path
    }

    #[test]
    fn accepts_backups_and_csv() {
        let json = temp_file("linguarust-import-test.json");
        let csv = temp_file("linguarust-import-test.CSV");
        assert!(importable_file(&json));
        assert!(importable_file(&csv), "регистр расширения не должен мешать");
        let _ = std::fs::remove_file(&json);
        let _ = std::fs::remove_file(&csv);
    }

    #[test]
    fn rejects_other_files() {
        let text = temp_file("linguarust-import-test.txt");
        assert!(!importable_file(&text));
        assert!(
            !importable_file(Path::new("C:/нет/такого.json")),
            "файла нет"
        );
        assert!(!importable_file(Path::new("отчёт")), "это не путь");
        let _ = std::fs::remove_file(&text);
    }

    #[test]
    fn rejects_huge_files() {
        let path = std::env::temp_dir().join("linguarust-import-huge.json");
        let file = std::fs::File::create(&path).expect("временный файл");
        file.set_len(MAX_DROPPED_FILE + 1)
            .expect("увеличиваем размер");
        assert!(!importable_file(&path), "файл больше предела");
        let _ = std::fs::remove_file(&path);
    }
}
