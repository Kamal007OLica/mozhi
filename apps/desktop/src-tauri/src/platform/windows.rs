//! Windows: WH_KEYBOARD_LL hook, foreground-window tracking, UI Automation focus,
//! clipboard + SendInput paste, mic privacy, and a topmost no-activate overlay.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use parking_lot::{Mutex, RwLock};
use windows::core::{w, BOOL, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetMonitorInfoW, MonitorFromPoint, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, UIA_ComboBoxControlTypeId, UIA_DocumentControlTypeId,
    UIA_EditControlTypeId, UIA_ValuePatternId,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CAPITAL, VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RETURN, VK_RMENU,
    VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::Shell::SHDefExtractIconW;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CallNextHookEx, DestroyIcon, DispatchMessageW, EnumWindows, GetCursorPos, GetForegroundWindow, GetIconInfo,
    GetMessageW, GetWindowLongPtrW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, SetWindowsHookExW, ShowWindow, TranslateMessage, GWL_EXSTYLE, HC_ACTION,
    HICON, HWND_TOPMOST, ICONINFO, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_HIDE, SW_RESTORE, SW_SHOWNOACTIVATE, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};

use super::detector::DoubleTap;
use super::{InsertOptions, KeyEvent};
use crate::types::{FocusedElement, FrontApp, HotkeyConfig, PermState, PermissionKind, Permissions, TriggerKey};

// ── Key hook ────────────────────────────────────────────────────────────────

struct Hook {
    hotkey: Arc<RwLock<HotkeyConfig>>,
    capture: Arc<AtomicBool>,
    tx: Mutex<mpsc::Sender<KeyEvent>>,
    state: Mutex<HookState>,
}

#[derive(Default)]
struct HookState {
    taps: DoubleTap,
    /// Modifier keys currently held (by vk), tracked from the hook itself.
    held: HashSet<u16>,
    /// Keys whose key-down we swallowed — swallow the matching key-up too.
    swallowed: HashSet<u16>,
}

static HOOK: OnceLock<Hook> = OnceLock::new();

const MODIFIER_VKS: [VIRTUAL_KEY; 13] = [
    VK_LCONTROL, VK_RCONTROL, VK_CONTROL, VK_LMENU, VK_RMENU, VK_MENU, VK_LSHIFT, VK_RSHIFT, VK_SHIFT, VK_LWIN, VK_RWIN, VK_CAPITAL,
    VIRTUAL_KEY(0xFF),
];

fn trigger_vks(key: TriggerKey) -> &'static [VIRTUAL_KEY] {
    match key {
        TriggerKey::Control => &[VK_LCONTROL, VK_RCONTROL],
        TriggerKey::RightControl => &[VK_RCONTROL],
        TriggerKey::Option => &[VK_LMENU, VK_RMENU],
        TriggerKey::RightOption => &[VK_RMENU],
        TriggerKey::Fn => &[], // Fn never reaches the OS on Windows keyboards
    }
}

impl Hook {
    /// Returns true when the event must be swallowed.
    fn handle(&self, msg: u32, kb: &KBDLLHOOKSTRUCT) -> bool {
        if kb.flags.0 & LLKHF_INJECTED.0 != 0 {
            return false; // our own SendInput, or another tool's
        }
        let vk = kb.vkCode as u16;
        let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        let now = Instant::now();
        let cfg = self.hotkey.read().clone();
        let mut st = self.state.lock();

        let is_trigger = trigger_vks(cfg.key).iter().any(|k| k.0 == vk);
        let is_modifier = MODIFIER_VKS.iter().any(|k| k.0 == vk);

        if is_trigger {
            if down {
                let repeat = !st.held.insert(vk);
                if !repeat {
                    let others = st.held.iter().any(|h| *h != vk);
                    st.taps.trigger_down(now, others, &cfg);
                }
            } else if up {
                st.held.remove(&vk);
                if st.taps.trigger_up(now, &cfg) {
                    let _ = self.tx.lock().send(KeyEvent::DoubleTap);
                }
            }
            return false;
        }
        if is_modifier {
            if down {
                if st.held.insert(vk) {
                    st.taps.other_input();
                }
            } else if up {
                st.held.remove(&vk);
            }
            return false;
        }
        if down {
            st.taps.other_input();
            let plain = st.held.is_empty();
            if plain && self.capture.load(Ordering::SeqCst) && (vk == VK_RETURN.0 || vk == VK_ESCAPE.0) {
                if st.swallowed.insert(vk) {
                    let ev = if vk == VK_ESCAPE.0 { KeyEvent::Escape } else { KeyEvent::Enter };
                    let _ = self.tx.lock().send(ev);
                }
                return true;
            }
        } else if up && st.swallowed.remove(&vk) {
            return true;
        }
        false
    }
}

unsafe extern "system" fn ll_keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        if let Some(hook) = HOOK.get() {
            // SAFETY: for WH_KEYBOARD_LL, lparam points to a KBDLLHOOKSTRUCT.
            let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            if hook.handle(wparam.0 as u32, kb) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub fn start_key_listener(
    hotkey: Arc<RwLock<HotkeyConfig>>,
    capture: Arc<AtomicBool>,
    on_event: Box<dyn Fn(KeyEvent) + Send + Sync>,
) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel::<KeyEvent>();
    if HOOK.set(Hook { hotkey, capture, tx: Mutex::new(tx), state: Mutex::new(HookState::default()) }).is_err() {
        anyhow::bail!("key listener already started");
    }
    thread::Builder::new().name("mozhi-keys".into()).spawn(move || {
        for ev in rx {
            on_event(ev);
        }
    })?;
    thread::Builder::new().name("mozhi-keyhook".into()).spawn(|| unsafe {
        let module = GetModuleHandleW(None).ok().map(|m| m.into());
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard), module, 0) {
            Ok(h) => h,
            Err(e) => {
                log::error!("could not install keyboard hook: {e}");
                return;
            }
        };
        log::info!("keyboard hook active ({hook:?})");
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    })?;
    Ok(())
}

// ── Foreground app ──────────────────────────────────────────────────────────

fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(handle);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// The exe's FileDescription ("Slack", "Google Chrome"), cached per path.
fn file_description(path: &str) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().get(path) {
        return hit.clone();
    }
    let desc = unsafe {
        let wpath = HSTRING::from(path);
        let size = GetFileVersionInfoSizeW(&wpath, None);
        let mut out = None;
        if size > 0 {
            let mut data = vec![0u8; size as usize];
            if GetFileVersionInfoW(&wpath, None, size, data.as_mut_ptr() as *mut c_void).is_ok() {
                let mut ptr: *mut c_void = std::ptr::null_mut();
                let mut len = 0u32;
                let mut langs = vec![(0x0409u16, 0x04B0u16)];
                if VerQueryValueW(data.as_ptr() as *const c_void, w!("\\VarFileInfo\\Translation"), &mut ptr, &mut len).as_bool()
                    && len >= 4
                {
                    let pairs = std::slice::from_raw_parts(ptr as *const u16, (len / 2) as usize);
                    langs.insert(0, (pairs[0], pairs[1]));
                }
                for (lang, cp) in langs {
                    let key = HSTRING::from(format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription"));
                    if VerQueryValueW(data.as_ptr() as *const c_void, &key, &mut ptr, &mut len).as_bool() && len > 1 {
                        let s = wide_to_string(std::slice::from_raw_parts(ptr as *const u16, len as usize));
                        if !s.trim().is_empty() {
                            out = Some(s.trim().to_string());
                            break;
                        }
                    }
                }
            }
        }
        out
    };
    cache.lock().insert(path.to_string(), desc.clone());
    desc
}

fn window_title(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    (n > 0).then(|| String::from_utf16_lossy(&buf[..n as usize])).filter(|s| !s.trim().is_empty())
}

fn pretty_stem(path: &str) -> String {
    let stem = Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("App");
    let mut c = stem.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "App".into(),
    }
}

pub fn frontmost_app() -> Option<FrontApp> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let path = process_path(pid);
        let title = window_title(hwnd);
        let exe = path
            .as_deref()
            .and_then(|p| Path::new(p).file_name())
            .and_then(|f| f.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| format!("pid:{pid}"));
        // UWP apps are hosted by ApplicationFrameHost — the window title names the real app.
        let name = if exe == "applicationframehost.exe" {
            title.clone().unwrap_or_else(|| "App".into())
        } else {
            path.as_deref().and_then(file_description).or_else(|| path.as_deref().map(pretty_stem)).unwrap_or_else(|| exe.clone())
        };
        Some(FrontApp { name, id: exe, pid: pid as i32, window_title: title, icon: None, is_self: pid == GetCurrentProcessId() })
    }
}

pub fn app_icon(app: &FrontApp) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().get(&app.id) {
        return hit.clone();
    }
    let icon = process_path(app.pid as u32).and_then(|p| exe_icon_png(&p, 64));
    cache.lock().insert(app.id.clone(), icon.clone());
    icon
}

fn exe_icon_png(path: &str, size: u32) -> Option<String> {
    unsafe {
        let mut hicon = HICON::default();
        let wpath = HSTRING::from(path);
        SHDefExtractIconW(&wpath, 0, 0, Some(&mut hicon), None, size).ok().ok()?;
        if hicon.is_invalid() {
            return None;
        }
        let rgba = icon_rgba(hicon, size);
        let _ = DestroyIcon(hicon);
        let rgba = rgba?;
        let mut png_bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_bytes, size, size);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().ok()?;
            writer.write_image_data(&rgba).ok()?;
        }
        Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png_bytes)))
    }
}

unsafe fn icon_rgba(hicon: HICON, size: u32) -> Option<Vec<u8>> {
    let mut info = ICONINFO::default();
    unsafe { GetIconInfo(hicon, &mut info) }.ok()?;
    let dc = unsafe { CreateCompatibleDC(None) };
    let read = |bitmap| -> Option<Vec<u8>> {
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (size * size * 4) as usize];
        let lines = unsafe { GetDIBits(dc, bitmap, 0, size, Some(buf.as_mut_ptr() as *mut c_void), &mut bmi, DIB_RGB_COLORS) };
        (lines > 0).then_some(buf)
    };
    let color = if info.hbmColor.is_invalid() { None } else { read(info.hbmColor) };
    let mask = if info.hbmMask.is_invalid() { None } else { read(info.hbmMask) };
    unsafe {
        let _ = DeleteDC(dc);
        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
    }
    let mut px = color?;
    let pixels = px.len() / 4;
    let has_alpha = (0..pixels).any(|i| px[i * 4 + 3] != 0);
    for i in 0..pixels {
        let o = i * 4;
        px.swap(o, o + 2); // BGRA → RGBA
        if !has_alpha {
            // Legacy icon: the AND mask says which pixels are transparent.
            let transparent = mask.as_ref().map(|m| m[o] != 0).unwrap_or(false);
            px[o + 3] = if transparent { 0 } else { 255 };
        }
    }
    Some(px)
}

// ── Focused element (UI Automation on a dedicated MTA thread) ───────────────

type UiaReply = mpsc::Sender<Option<FocusedElement>>;

fn uia_worker() -> Option<&'static Mutex<mpsc::Sender<UiaReply>>> {
    static WORKER: OnceLock<Option<Mutex<mpsc::Sender<UiaReply>>>> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel::<UiaReply>();
            thread::Builder::new()
                .name("mozhi-uia".into())
                .spawn(move || unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                    let automation: Option<IUIAutomation> = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
                    for reply in rx {
                        let el = automation.as_ref().and_then(|a| read_focused(a));
                        let _ = reply.send(el);
                    }
                })
                .ok()?;
            Some(Mutex::new(tx))
        })
        .as_ref()
}

unsafe fn read_focused(automation: &IUIAutomation) -> Option<FocusedElement> {
    let el = unsafe { automation.GetFocusedElement() }.ok()?;
    let control = unsafe { el.CurrentControlType() }.ok()?;
    let name = unsafe { el.CurrentName() }.ok().map(|b| b.to_string()).filter(|s| !s.trim().is_empty());
    let value_editable = unsafe { el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
        .ok()
        .and_then(|p| unsafe { p.CurrentIsReadOnly() }.ok())
        .map(|ro| !ro.as_bool())
        .unwrap_or(false);
    let role = if control == UIA_EditControlTypeId {
        if name.as_deref().map(|n| n.to_lowercase().contains("search")).unwrap_or(false) {
            "searchField"
        } else {
            "textField"
        }
    } else if control == UIA_DocumentControlTypeId {
        "textArea"
    } else if control == UIA_ComboBoxControlTypeId {
        "comboBox"
    } else {
        "other"
    };
    let editable = value_editable || matches!(role, "textField" | "searchField" | "textArea");
    let label = name.map(|s| {
        let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if s.chars().count() > 80 {
            s.chars().take(79).collect::<String>() + "…"
        } else {
            s
        }
    });
    Some(FocusedElement { role: role.into(), label, editable })
}

pub fn focused_element() -> Option<FocusedElement> {
    static BUSY: AtomicBool = AtomicBool::new(false);
    if BUSY.swap(true, Ordering::SeqCst) {
        return None; // previous query still stuck in a slow app; don't pile up
    }
    let result = (|| {
        let worker = uia_worker()?;
        let (tx, rx) = mpsc::channel();
        worker.lock().send(tx).ok()?;
        rx.recv_timeout(Duration::from_millis(60)).ok().flatten()
    })();
    BUSY.store(false, Ordering::SeqCst);
    result
}

// ── Clipboard & insertion ───────────────────────────────────────────────────

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    arboard::Clipboard::new()?.set_text(text.to_string())?;
    Ok(())
}

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 },
        },
    }
}

fn foreground_pid() -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
    pid
}

struct FindWindow {
    pid: u32,
    title: Option<String>,
    found: Option<HWND>,
}

unsafe extern "system" fn find_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = unsafe { &mut *(lparam.0 as *mut FindWindow) };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid != search.pid || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return BOOL(1);
    }
    let title = window_title(hwnd);
    if search.found.is_none() || (search.title.is_some() && title == search.title) {
        search.found = Some(hwnd);
        if search.title.is_none() || title == search.title {
            return BOOL(0); // exact match — stop
        }
    }
    BOOL(1)
}

fn bring_to_front(target: &FrontApp) -> anyhow::Result<()> {
    let mut search = FindWindow { pid: target.pid as u32, title: target.window_title.clone(), found: None };
    unsafe {
        let _ = EnumWindows(Some(find_window), LPARAM(&mut search as *mut FindWindow as isize));
    }
    let hwnd = search.found.ok_or_else(|| anyhow::anyhow!("{} has no open window", target.name))?;
    unsafe {
        let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(me, fg_thread, false);
        }
    }
    let deadline = Instant::now() + Duration::from_millis(300);
    while foreground_pid() != target.pid as u32 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(15));
    }
    thread::sleep(Duration::from_millis(40));
    Ok(())
}

pub fn insert_text(text: &str, target: &FrontApp, opts: InsertOptions) -> anyhow::Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    let previous = if opts.restore_clipboard { clipboard.get_text().ok() } else { None };

    if foreground_pid() != target.pid as u32 {
        bring_to_front(target)?;
    }

    if opts.restore_clipboard {
        use arboard::SetExtWindows;
        clipboard.set().exclude_from_history().exclude_from_cloud().text(text.to_string())?;
    } else {
        clipboard.set_text(text.to_string())?;
    }
    let ours = unsafe { GetClipboardSequenceNumber() };

    // Release modifiers the user may still be holding (the trigger), then Ctrl+V.
    let mut inputs = Vec::new();
    for vk in [VK_LCONTROL, VK_RCONTROL, VK_LMENU, VK_RMENU, VK_LSHIFT, VK_RSHIFT, VK_LWIN, VK_RWIN] {
        if unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000 != 0 {
            inputs.push(key_input(vk, true));
        }
    }
    inputs.extend([key_input(VK_CONTROL, false), key_input(VK_V, false), key_input(VK_V, true), key_input(VK_CONTROL, true)]);
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        anyhow::bail!("paste was blocked (is {} running as administrator?)", target.name);
    }

    if let Some(previous) = previous {
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(350));
            if unsafe { GetClipboardSequenceNumber() } == ours {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(previous);
                }
            }
        });
    }
    Ok(())
}

// ── Permissions ─────────────────────────────────────────────────────────────

/// Windows' microphone privacy switch (Settings → Privacy → Microphone → desktop apps).
fn mic_state() -> PermState {
    let read = |subkey: PCWSTR| -> Option<String> {
        let mut buf = [0u16; 32];
        let mut len = (buf.len() * 2) as u32;
        let err = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, subkey, w!("Value"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut c_void), Some(&mut len))
        };
        err.is_ok().then(|| wide_to_string(&buf))
    };
    let global = read(w!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone"));
    let desktop =
        read(w!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone\\NonPackaged"));
    match (global.as_deref(), desktop.as_deref()) {
        (Some("Deny"), _) | (_, Some("Deny")) => PermState::Denied,
        _ => PermState::Granted,
    }
}

pub fn permissions() -> Permissions {
    Permissions { microphone: mic_state(), accessibility: PermState::NotRequired }
}

pub fn request_permission(kind: PermissionKind) {
    if kind == PermissionKind::Microphone && mic_state() == PermState::Denied {
        open_permission_settings(kind);
    }
}

pub fn open_permission_settings(kind: PermissionKind) {
    if kind == PermissionKind::Microphone {
        if let Err(e) = std::process::Command::new("explorer").arg("ms-settings:privacy-microphone").spawn() {
            log::warn!("open settings: {e}");
        }
    }
}

// ── Overlay ─────────────────────────────────────────────────────────────────

const BOTTOM_MARGIN: f64 = 72.0;

#[derive(Clone, Copy)]
struct Hwnd(isize);

impl Hwnd {
    fn of(window: &tauri::WebviewWindow) -> anyhow::Result<Self> {
        Ok(Self(window.hwnd()?.0 as isize))
    }
    fn get(self) -> HWND {
        HWND(self.0 as *mut c_void)
    }
}

pub fn setup_overlay(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    let h = Hwnd::of(window)?;
    window.run_on_main_thread(move || unsafe {
        let hwnd = h.get();
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let add = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0) as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | add);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    })?;
    Ok(())
}

pub fn show_overlay(window: &tauri::WebviewWindow, position: Option<(f64, f64)>) -> anyhow::Result<()> {
    if let Some((x, y)) = position {
        window.set_position(tauri::LogicalPosition::new(x, y))?;
    }
    let h = Hwnd::of(window)?;
    window.run_on_main_thread(move || unsafe {
        let hwnd = h.get();
        if position.is_none() {
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let mut rect = RECT::default();
            if GetMonitorInfoW(monitor, &mut info).as_bool() && GetWindowRect(hwnd, &mut rect).is_ok() {
                let scale = GetDpiForWindow(hwnd).max(96) as f64 / 96.0;
                let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
                let work = info.rcWork;
                let x = work.left + ((work.right - work.left) - w) / 2;
                let y = work.bottom - h - (BOTTOM_MARGIN * scale).round() as i32;
                let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
    })?;
    Ok(())
}

pub fn hide_overlay(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    let h = Hwnd::of(window)?;
    window.run_on_main_thread(move || unsafe {
        let _ = ShowWindow(h.get(), SW_HIDE);
    })?;
    Ok(())
}

pub fn set_dock_visible(_app: &tauri::AppHandle, _visible: bool) {}
