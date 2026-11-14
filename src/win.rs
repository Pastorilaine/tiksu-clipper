//! Small Win32 helpers: single instance, GPU name, process list, autostart, recycle bin.

use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, INVALID_HANDLE_VALUE};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Named event that both detects a running instance and asks it to show its window.
/// `None`: another instance is running and has been signalled, so this one should exit.
pub fn single_instance() -> Option<SingleInstance> {
    use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};
    // A test/staging run (custom update source) must not hand over to the real instance.
    let test = std::env::var_os("TIKSU_UPDATE_SOURCE").is_some();
    let name = wide(if test { "Local\\TiksuClipper.Show.test" } else { "Local\\TiksuClipper.Show" });
    unsafe {
        let handle = CreateEventW(std::ptr::null(), 0, 0, name.as_ptr());
        if !handle.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            SetEvent(handle);
            CloseHandle(handle);
            return None;
        }
        Some(SingleInstance(handle as isize))
    }
}

pub struct SingleInstance(isize);

impl SingleInstance {
    /// Calls `on_show` every time a second instance is started.
    pub fn on_show_request(self, on_show: impl Fn() + Send + 'static) {
        use windows_sys::Win32::System::Threading::{WaitForSingleObject, INFINITE};
        if self.0 == 0 {
            return;
        }
        std::thread::spawn(move || unsafe {
            while WaitForSingleObject(self.0 as HANDLE, INFINITE) == 0 {
                on_show();
            }
        });
    }
}

/// Name of the GPU driving the primary display. Instant, unlike WMI.
pub fn gpu_name() -> String {
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_PRIMARY_DEVICE};
    let mut first = None;
    for i in 0.. {
        let mut dd: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), i, &mut dd, 0) } == 0 {
            break;
        }
        let name = from_wide(&dd.DeviceString);
        if dd.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0 {
            return name;
        }
        first.get_or_insert(name);
    }
    first.unwrap_or_else(|| "Tuntematon näytönohjain".to_string())
}

/// (pid, parent pid, exe name) of every running process.
pub fn processes() -> Vec<(u32, u32, String)> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut procs = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return procs;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        while ok {
            procs.push((entry.th32ProcessID, entry.th32ParentProcessID, from_wide(&entry.szExeFile)));
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
    }
    procs
}

/// Free bytes on the drive holding `path` (the nearest existing parent is used).
pub fn free_disk_bytes(path: &Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let dir = path.ancestors().find(|p| p.exists())?;
    let wide_dir = wide(&dir.to_string_lossy());
    let mut free = 0u64;
    let ok = unsafe { GetDiskFreeSpaceExW(wide_dir.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

/// Physical memory currently available, in bytes.
pub fn available_ram_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullAvailPhys)
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "TiksuClipper";

/// Starts the app (to the tray) at Windows sign-in, or stops doing so.
pub fn set_autostart(enable: bool) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
    let status = unsafe {
        if enable {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let cmd = wide(&format!("\"{}\" --tray", exe.display()));
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                REG_SZ,
                cmd.as_ptr().cast(),
                (cmd.len() * 2) as u32,
            )
        } else {
            match RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) {
                2 => 0, // ERROR_FILE_NOT_FOUND: already off
                s => s,
            }
        }
    };
    if status == 0 { Ok(()) } else { Err(format!("rekisterivirhe {}", status)) }
}

/// Moves a file to the recycle bin, so a deleted clip can still be restored.
pub fn recycle(path: &Path) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
    };
    // pFrom is a double-null-terminated list.
    let mut from = wide(&path.to_string_lossy());
    from.push(0);
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = FO_DELETE;
    op.pFrom = from.as_ptr();
    op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI) as u16;
    match unsafe { SHFileOperationW(&mut op) } {
        0 if op.fAnyOperationsAborted == 0 => Ok(()),
        code => Err(format!("poisto epäonnistui (koodi {})", code)),
    }
}

/// Puts a file on the clipboard the way Explorer's Copy does (CF_HDROP), so Ctrl+V pastes the file
/// itself into Discord, a chat or a folder.
pub fn copy_file_to_clipboard(path: &Path) -> Result<(), String> {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows_sys::Win32::UI::Shell::DROPFILES;
    const CF_HDROP: u32 = 15;

    // DROPFILES header, then the paths as a double-null-terminated UTF-16 list.
    let mut list = wide(&path.to_string_lossy());
    list.push(0);
    let header = std::mem::size_of::<DROPFILES>();
    let size = header + list.len() * 2;
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, size);
        if mem.is_null() {
            return Err("muistia ei saatu".to_string());
        }
        let ptr = GlobalLock(mem) as *mut u8;
        if ptr.is_null() {
            GlobalFree(mem);
            return Err("muistia ei saatu".to_string());
        }
        let mut drop: DROPFILES = std::mem::zeroed();
        drop.pFiles = header as u32;
        drop.fWide = 1;
        std::ptr::write_unaligned(ptr as *mut DROPFILES, drop);
        std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, ptr.add(header), list.len() * 2);
        GlobalUnlock(mem);

        if OpenClipboard(std::ptr::null_mut()) == 0 {
            GlobalFree(mem);
            return Err("leikepöytä on toisen ohjelman käytössä".to_string());
        }
        EmptyClipboard();
        // On success the clipboard owns the memory.
        let ok = !SetClipboardData(CF_HDROP, mem).is_null();
        CloseClipboard();
        if !ok {
            GlobalFree(mem);
            return Err("leikepöydälle kirjoitus epäonnistui".to_string());
        }
    }
    Ok(())
}

/// Waits (up to `timeout_ms`) for a process to exit, e.g. the old version handing over after an update.
pub fn wait_for_exit(pid: u32, timeout_ms: u32) {
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !handle.is_null() {
            WaitForSingleObject(handle, timeout_ms);
            CloseHandle(handle);
        }
    }
}

/// Colours the native title bar to match the app (Windows 11: caption, text and border colours;
/// Windows 10 only gets the dark mode flag). Cheap to call every frame: the work is done once, as soon
/// as the window exists.
pub fn style_titlebar(bg: eframe::egui::Color32, text: eframe::egui::Color32, border: eframe::egui::Color32) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, GetWindowThreadProcessId};

    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.load(Ordering::Relaxed) {
        return;
    }

    struct Search {
        pid: u32,
        found: HWND,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let search = &mut *(lparam as *mut Search);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let mut title = [0u16; 32];
            let len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
            if pid == search.pid && String::from_utf16_lossy(&title[..len.max(0) as usize]) == "Tiksu Clipper" {
                search.found = hwnd;
                return 0;
            }
            1
        }
    }

    let mut search = Search { pid: unsafe { GetCurrentProcessId() }, found: std::ptr::null_mut() };
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
    if search.found.is_null() {
        return;
    }
    let colorref = |c: eframe::egui::Color32| c.r() as u32 | (c.g() as u32) << 8 | (c.b() as u32) << 16;
    let set = |attribute: u32, value: u32| unsafe {
        DwmSetWindowAttribute(search.found, attribute, (&value as *const u32).cast(), 4);
    };
    set(20, 1); // DWMWA_USE_IMMERSIVE_DARK_MODE
    set(34, colorref(border)); // DWMWA_BORDER_COLOR
    set(35, colorref(bg)); // DWMWA_CAPTION_COLOR
    set(36, colorref(text)); // DWMWA_TEXT_COLOR
    DONE.store(true, Ordering::Relaxed);
}

/// Name of the first hardware H.264 encoder Windows offers (NVENC, AMF, Quick Sync), or `None` when
/// video would be encoded on the CPU. The transcoder falls back to software without saying so.
pub fn hardware_h264_encoder() -> Option<String> {
    use windows::Win32::Media::MediaFoundation::{
        IMFActivate, MFMediaType_Video, MFStartup, MFShutdown, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE,
        MFT_ENUM_FLAG_SORTANDFILTER, MFT_FRIENDLY_NAME_Attribute, MFT_REGISTER_TYPE_INFO, MFVideoFormat_H264, MFSTARTUP_LITE,
        MF_VERSION, MFTEnumEx,
    };
    use windows::Win32::System::Com::CoTaskMemFree;
    unsafe {
        MFStartup(MF_VERSION, MFSTARTUP_LITE).ok()?;
        let output = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_H264 };
        let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        let found = MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&output),
            &mut activates,
            &mut count,
        );
        let mut name = None;
        if found.is_ok() && !activates.is_null() {
            let items = std::slice::from_raw_parts_mut(activates, count as usize);
            name = items.first().and_then(|a| a.as_ref()).and_then(|a| {
                let mut buf = [0u16; 256];
                let mut len = 0u32;
                a.GetString(&MFT_FRIENDLY_NAME_Attribute, &mut buf, Some(&mut len)).ok()?;
                Some(String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]))
            });
            for item in items.iter_mut() {
                *item = None; // releases the COM reference
            }
            CoTaskMemFree(Some(activates as *const _));
        }
        let _ = MFShutdown();
        name.or_else(|| (count > 0).then(|| "Laitteistoenkooderi".to_string()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hardware_encoder_query_runs() {
        // Machine dependent: a CI runner has none. It must just not crash.
        println!("hardware encoder: {:?}", super::hardware_h264_encoder());
    }

    /// Overwrites the clipboard, so manual: `cargo test clipboard -- --ignored`
    #[test]
    #[ignore]
    fn clipboard_holds_the_file() {
        let file = std::env::temp_dir().join("tiksu clipboard ä.mp4");
        std::fs::write(&file, b"x").unwrap();
        super::copy_file_to_clipboard(&file).unwrap();
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "[Console]::OutputEncoding=[Text.Encoding]::UTF8; (Get-Clipboard -Format FileDropList).FullName"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), file.to_string_lossy());
    }
}
