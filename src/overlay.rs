//! Small always-on-top toast in the screen corner ("Leike tallennettu"), visible over borderless
//! games. Plain Win32 on its own thread, so it works while the main window is hidden in the tray.

use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const W: i32 = 360;
const H: i32 = 64;
const MARGIN: i32 = 24;
const SHOW_MS: u32 = 2600;
const WM_APP_SHOW: u32 = WM_APP + 1;

static HWND_: AtomicIsize = AtomicIsize::new(0);
static STARTED: OnceLock<()> = OnceLock::new();
/// (title, subtitle, success)
static TEXT: Mutex<(String, String, bool)> = Mutex::new((String::new(), String::new(), true));

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Shows the toast for a moment. Safe to call from any thread.
pub fn show(title: &str, subtitle: &str, success: bool) {
    STARTED.get_or_init(|| {
        std::thread::spawn(run);
    });
    *TEXT.lock().unwrap() = (title.to_string(), subtitle.to_string(), success);
    // The window is created asynchronously on first use; wait briefly for it.
    for _ in 0..50 {
        let hwnd = HWND_.load(Ordering::SeqCst);
        if hwnd != 0 {
            unsafe { PostMessageW(hwnd as HWND, WM_APP_SHOW, 0, 0) };
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn run() {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = wide("TiksuClipperOverlay");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            W,
            H,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return;
        }
        SetLayeredWindowAttributes(hwnd, 0, 240, LWA_ALPHA);
        SetWindowRgn(hwnd, CreateRoundRectRgn(0, 0, W + 1, H + 1, 20, 20), 1);
        HWND_.store(hwnd as isize, Ordering::SeqCst);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SHOW => {
                let x = GetSystemMetrics(SM_CXSCREEN) - W - MARGIN;
                SetWindowPos(hwnd, HWND_TOPMOST, x, MARGIN, W, H, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                InvalidateRect(hwnd, std::ptr::null(), 1);
                SetTimer(hwnd, 1, SHOW_MS, None);
                0
            }
            WM_TIMER => {
                KillTimer(hwnd, 1);
                ShowWindow(hwnd, SW_HIDE);
                0
            }
            WM_PAINT => {
                paint(hwnd);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn paint(hwnd: HWND) {
    unsafe {
        let (title, subtitle, success) = TEXT.lock().unwrap().clone();
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);

        let bg = CreateSolidBrush(rgb(0x0d, 0x1a, 0x22));
        let full = RECT { left: 0, top: 0, right: W, bottom: H };
        FillRect(dc, &full, bg);
        DeleteObject(bg);
        let accent = CreateSolidBrush(if success { rgb(0x3a, 0xd9, 0xb8) } else { rgb(0xff, 0x6b, 0x7d) });
        FillRect(dc, &RECT { left: 0, top: 0, right: 5, bottom: H }, accent);
        DeleteObject(accent);

        SetBkMode(dc, TRANSPARENT as i32);
        let face = wide("Segoe UI");
        let font = |size: i32, weight: i32| {
            CreateFontW(-size, 0, 0, 0, weight, 0, 0, 0, DEFAULT_CHARSET as u32, 0, 0, CLEARTYPE_QUALITY as u32, 0, face.as_ptr())
        };
        let (big, small) = (font(18, 600), font(14, 400));

        let mut title_w = wide(&title);
        SelectObject(dc, big);
        SetTextColor(dc, rgb(0xea, 0xf2, 0xf7));
        let mut r = RECT { left: 20, top: 10, right: W - 12, bottom: 34 };
        DrawTextW(dc, title_w.as_mut_ptr(), -1, &mut r, DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);

        let mut sub_w = wide(&subtitle);
        SelectObject(dc, small);
        SetTextColor(dc, rgb(0xb9, 0xcb, 0xd6));
        let mut r = RECT { left: 20, top: 34, right: W - 12, bottom: H - 8 };
        DrawTextW(dc, sub_w.as_mut_ptr(), -1, &mut r, DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);

        DeleteObject(big);
        DeleteObject(small);
        EndPaint(hwnd, &ps);
    }
}
