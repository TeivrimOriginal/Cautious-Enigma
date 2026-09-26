//! Тонкие вызовы Win32 для поведения, которого нет в кроссплатформенном tao.
//!
//! `Window::set_minimized` в tao меняет только внутренние флаги окна, поэтому
//! настоящее сворачивание, восстановление и вывод на передний план делаются
//! через `ShowWindow` и `SetForegroundWindow`.

use tao::window::Window;

/// Сворачивает окно.
pub fn minimize(window: &Window) {
    #[cfg(windows)]
    {
        use tao::platform::windows::WindowExtWindows;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_MINIMIZE, ShowWindow};

        unsafe {
            ShowWindow(window.hwnd() as _, SW_MINIMIZE);
        }
    }
    #[cfg(not(windows))]
    window.set_minimized(true);
}

/// Восстанавливает окно и ставит его на передний план.
pub fn restore(window: &Window) {
    #[cfg(windows)]
    {
        use tao::platform::windows::WindowExtWindows;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SW_RESTORE, SetForegroundWindow, ShowWindow,
        };

        unsafe {
            ShowWindow(window.hwnd() as _, SW_RESTORE);
            SetForegroundWindow(window.hwnd() as _);
        }
    }
    #[cfg(not(windows))]
    window.set_minimized(false);
}

/// Прячет окно без появления в панели задач — режим «трей».
pub fn hide(window: &Window) {
    #[cfg(windows)]
    {
        use tao::platform::windows::WindowExtWindows;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

        unsafe {
            ShowWindow(window.hwnd() as _, SW_HIDE);
        }
    }
    #[cfg(not(windows))]
    window.set_visible(false);
}
