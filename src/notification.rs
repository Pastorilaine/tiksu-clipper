/// Windows notification sound: "asterisk" for a saved clip, "error" for a failure.
pub fn play_chime(success: bool) {
    use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONASTERISK, MB_ICONHAND};
    unsafe {
        MessageBeep(if success { MB_ICONASTERISK } else { MB_ICONHAND });
    }
}
