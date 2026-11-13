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
