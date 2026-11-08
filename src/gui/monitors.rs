//! Work area of the monitor that contains a physical point.
//!
//! Tray icon rectangles are in virtual-desktop physical pixels. The main
//! window's monitor size is not that coordinate system: a panel clamped
//! against it jumps when the window moves to another display.

/// Work area in physical pixels, virtual-desktop origin. Excludes the taskbar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorkArea {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

pub(crate) fn work_area_at(x: f64, y: f64) -> Option<WorkArea> {
    #[cfg(windows)]
    {
        work_area_at_windows(x, y)
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y);
        None
    }
}

#[cfg(windows)]
fn work_area_at_windows(x: f64, y: f64) -> Option<WorkArea> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };

    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: x.round() as i32,
                y: y.round() as i32,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        if monitor.is_invalid() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let work = info.rcWork;
        let width = work.right.saturating_sub(work.left);
        let height = work.bottom.saturating_sub(work.top);
        if width < 1 || height < 1 {
            return None;
        }
        Some(WorkArea {
            x: work.left as f32,
            y: work.top as f32,
            width: width as f32,
            height: height as f32,
        })
    }
}
