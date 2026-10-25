//! Where the main window opens. Ported from DriverStore Manager (`settings::fit_window`), same rule and same tests.
//! Windows asks the monitor for its work area before the window exists; elsewhere the screen is only known once
//! the window is open (see `ui::more::track_window`), so there the rule is applied to the monitor size.

/// Where the main window opens. `saved` is the window rectangle of the last run (left, top, width, height) and
/// `work` the work area of the monitor it would appear on (left, top, right, bottom).
///  1. The saved rectangle is kept when it fits entirely inside the work area. `tolerance` points may stick
///     out: Windows 10 and 11 give a window an invisible border of a few pixels, so a window placed against the
///     edge of the screen has a rectangle that is slightly outside it.
///  2. Otherwise, and on the first run, the window is 60% of the work area wide and tall, centered.
pub fn fit_window(saved: Option<(i32, i32, i32, i32)>, work: (i32, i32, i32, i32), tolerance: i32) -> (i32, i32, i32, i32) {
    let (work_left, work_top, work_right, work_bottom) = work;
    if let Some((left, top, width, height)) = saved {
        let fits = width > 0
            && height > 0
            && left >= work_left - tolerance
            && top >= work_top - tolerance
            && left + width <= work_right + tolerance
            && top + height <= work_bottom + tolerance;
        if fits {
            return (left, top, width, height);
        }
    }
    let (work_width, work_height) = (work_right - work_left, work_bottom - work_top);
    let width = (work_width * 6 / 10).max(1);
    let height = (work_height * 6 / 10).max(1);
    (work_left + (work_width - width) / 2, work_top + (work_height - height) / 2, width, height)
}

/// Work area (left, top, right, bottom), in points, of the monitor a rectangle is on, or of the nearest one; of the
/// main monitor when there is no rectangle. None when it cannot be known before the window exists.
#[cfg(windows)]
pub fn work_area(rect: Option<(i32, i32, i32, i32)>) -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MonitorFromRect, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY};
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForSystem, MDT_EFFECTIVE_DPI};
    let system_k = unsafe { GetDpiForSystem() }.max(96) as f32 / 96.0;
    // The program is per-monitor DPI aware: points are scaled by the DPI of the monitor the window is on, which can differ
    // from the system one. Pick the monitor with a scale, ask for the scale of that monitor, and pick again with it.
    let pick = |k: f32| -> HMONITOR {
        unsafe {
            match rect {
                Some((x, y, w, h)) => {
                    let r = RECT { left: (x as f32 * k) as i32, top: (y as f32 * k) as i32, right: ((x + w) as f32 * k) as i32, bottom: ((y + h) as f32 * k) as i32 };
                    MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST)
                }
                None => MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY),
            }
        }
    };
    let scale_of = |m: HMONITOR, fallback: f32| -> f32 {
        let (mut dx, mut dy) = (0u32, 0u32);
        if unsafe { GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) } == 0 && dx > 0 { dx as f32 / 96.0 } else { fallback }
    };
    let mut k = system_k;
    let mut monitor = pick(k);
    for _ in 0..2 {
        let next = scale_of(monitor, k);
        if (next - k).abs() < 0.01 { break; }
        k = next;
        monitor = pick(k);
    }
    k = scale_of(monitor, k);
    unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) != 0 && info.rcWork.right > info.rcWork.left && info.rcWork.bottom > info.rcWork.top {
            let w = info.rcWork;
            return Some(((w.left as f32 / k) as i32, (w.top as f32 / k) as i32, (w.right as f32 / k) as i32, (w.bottom as f32 / k) as i32));
        }
    }
    None
}

#[cfg(not(windows))]
pub fn work_area(_rect: Option<(i32, i32, i32, i32)>) -> Option<(i32, i32, i32, i32)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_fit_rule() {
        // Saved on a 1920x1080 screen, opened on a 1024x728 work area: does not fit, 60% centered.
        assert_eq!(fit_window(Some((525, 246, 1640, 999)), (0, 0, 1024, 728), 16), (205, 146, 614, 436));
        // First run: 60% centered.
        assert_eq!(fit_window(None, (0, 0, 1920, 1040), 16), (384, 208, 1152, 624));
        // Fits: kept exactly, not centered.
        assert_eq!(fit_window(Some((100, 50, 1200, 700)), (0, 0, 1920, 1040), 16), (100, 50, 1200, 700));
        // Against the left edge with the invisible border of the window: kept.
        assert_eq!(fit_window(Some((-7, 0, 974, 1040)), (0, 0, 1920, 1040), 16), (-7, 0, 974, 1040));
        // Too far out, or too big in only one direction: not kept.
        assert_ne!(fit_window(Some((-40, 0, 974, 1040)), (0, 0, 1920, 1040), 16), (-40, 0, 974, 1040));
        assert_ne!(fit_window(Some((0, 0, 1200, 1100)), (0, 0, 1920, 1040), 16), (0, 0, 1200, 1100));
        // A monitor to the left of the main one (negative coordinates) and a taskbar at the top.
        assert_eq!(fit_window(None, (-1920, 0, 0, 1040), 16), (-1536, 208, 1152, 624));
        assert_eq!(fit_window(None, (0, 48, 1920, 1080), 16), (384, 254, 1152, 619));
        // Whatever the saved rectangle, the result is never outside the work area unless it was kept.
        let works = [(0, 0, 1920, 1040), (0, 0, 1024, 728), (0, 0, 800, 600), (-1920, 0, 0, 1040), (0, 48, 1920, 1080), (0, 0, 10, 10)];
        for work in works {
            for left in (-3000..3000).step_by(211) {
                for top in (-3000..3000).step_by(211) {
                    for (width, height) in [(1, 1), (400, 300), (1024, 700), (1640, 999), (20000, 20000)] {
                        let saved = (left, top, width, height);
                        let result = fit_window(Some(saved), work, 16);
                        if result != saved {
                            let (l, t, w, h) = result;
                            assert!(l >= work.0 && t >= work.1 && l + w <= work.2 && t + h <= work.3, "{saved:?} on {work:?} -> {result:?}");
                        }
                    }
                }
            }
        }
    }
}
