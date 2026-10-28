use std::{path::Path, process::Command};

pub fn pointer() -> ([f32; 2], bool) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::POINT,
            UI::{
                Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
                WindowsAndMessaging::GetCursorPos,
            },
        };
        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        return (
            [point.x as f32, point.y as f32],
            GetAsyncKeyState(VK_LBUTTON as i32) < 0,
        );
    }
    #[cfg(not(windows))]
    {
        ([0.0, 0.0], false)
    }
}

pub fn foreground_fullscreen() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::RECT,
            Graphics::Gdi::{
                GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
            },
            UI::WindowsAndMessaging::{
                GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect,
                GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, GWL_STYLE,
                WS_CAPTION,
            },
        };
        let window = GetForegroundWindow();
        if window.is_null() || IsIconic(window) != 0 || IsWindowVisible(window) == 0 {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        if pid == std::process::id() {
            return false;
        }
        if IsZoomed(window) != 0
            && GetWindowLongW(window, GWL_STYLE) as u32 & WS_CAPTION == WS_CAPTION
        {
            return false;
        }
        let mut rect: RECT = std::mem::zeroed();
        let mut monitor: MONITORINFO = std::mem::zeroed();
        monitor.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetWindowRect(window, &mut rect) == 0
            || GetMonitorInfoW(
                MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            ) == 0
        {
            return false;
        }
        let bounds = monitor.rcMonitor;
        let work = work_area();
        let center = [(work[0] + work[2]) * 0.5, (work[1] + work[3]) * 0.5];
        // Areas currently live on the primary display; fullscreen on another monitor must not hide them.
        if center[0] < bounds.left as f32
            || center[0] >= bounds.right as f32
            || center[1] < bounds.top as f32
            || center[1] >= bounds.bottom as f32
        {
            return false;
        }
        if !covers_monitor(
            [rect.left, rect.top, rect.right, rect.bottom],
            [bounds.left, bounds.top, bounds.right, bounds.bottom],
        ) {
            return false;
        }
        let mut class = [0u16; 128];
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        let class = String::from_utf16_lossy(&class[..length.max(0) as usize]);
        return !matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        );
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn covers_monitor(window: [i32; 4], monitor: [i32; 4]) -> bool {
    window[0] <= monitor[0] + 2
        && window[1] <= monitor[1] + 2
        && window[2] >= monitor[2] - 2
        && window[3] >= monitor[3] - 2
}

#[cfg(test)]
mod fullscreen_tests {
    use super::*;
    #[test]
    fn full_monitor_is_fullscreen_but_maximized_work_area_is_not() {
        let monitor = [0, 0, 1920, 1080];
        assert!(covers_monitor(monitor, monitor));
        assert!(covers_monitor([-8, -8, 1928, 1088], monitor));
        assert!(!covers_monitor([-8, -8, 1928, 1048], monitor));
        assert!(!covers_monitor([50, 50, 900, 700], monitor));
    }
}

pub fn quiet(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000 | 0x00004000);
    }
}

pub fn background_thread() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

pub fn reveal(path: &Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        let mut command = Command::new("explorer.exe");
        if path.is_file() {
            command.arg(format!("/select,{}", path.display()));
        } else {
            command.arg(path);
        }
        quiet(&mut command);
        command.spawn()?;
    }
    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(path.parent().unwrap_or(path))
            .spawn()?;
    }
    Ok(())
}

pub fn work_area() -> [f32; 4] {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::RECT,
            UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA},
        };
        let mut rect: RECT = std::mem::zeroed();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as *mut _, 0) != 0 {
            return [
                rect.left as f32,
                rect.top as f32,
                rect.right as f32,
                rect.bottom as f32,
            ];
        }
    }
    [0.0, 0.0, 1920.0, 1040.0]
}

#[cfg(windows)]
pub struct ProcessGroup(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl ProcessGroup {
    pub fn attach(child: &std::process::Child) -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return None;
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
            {
                CloseHandle(handle);
                return None;
            }
            Some(Self(handle))
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
