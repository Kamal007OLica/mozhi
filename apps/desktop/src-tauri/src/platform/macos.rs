//! macOS: CGEventTap key hook, NSWorkspace / Accessibility focus tracking, NSPasteboard +
//! synthetic ⌘V insertion, TCC permissions and the non-activating overlay panel.

#![allow(non_upper_case_globals)]

use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void, CStr};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use objc2::rc::autoreleasepool;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{class, msg_send, sel};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use parking_lot::{Mutex, RwLock};

use super::detector::DoubleTap;
use super::{InsertOptions, KeyEvent};
use crate::types::{FocusedElement, FrontApp, HotkeyConfig, PermState, PermissionKind, Permissions, TriggerKey};

// ── FFI ─────────────────────────────────────────────────────────────────────

type CGEventRef = *mut c_void;
type CFMachPortRef = *mut c_void;
type AXUIElementRef = *const c_void;
type TapCallback = extern "C" fn(*mut c_void, u32, CGEventRef, *mut c_void) -> CGEventRef;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(tap: u32, place: u32, options: u32, mask: u64, callback: TapCallback, info: *mut c_void) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventSourceCreate(state: i32) -> *mut c_void;
    fn CGEventCreateKeyboardEvent(source: *mut c_void, keycode: u16, down: bool) -> CGEventRef;
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGColorSpaceRelease(cs: *mut c_void);
    fn CGBitmapContextCreate(data: *mut c_void, w: usize, h: usize, bpc: usize, bpr: usize, cs: *mut c_void, info: u32) -> *mut c_void;
    fn CGBitmapContextCreateImage(ctx: *mut c_void) -> *mut c_void;
    fn CGContextSetInterpolationQuality(ctx: *mut c_void, q: i32);
    fn CGContextDrawImage(ctx: *mut c_void, rect: CGRect, image: *mut c_void);
    fn CGContextRelease(ctx: *mut c_void);
    fn CGImageRelease(image: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(alloc: *const c_void, port: CFMachPortRef, order: isize) -> *mut c_void;
    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: CFStringRef);
    fn CFRunLoopRun();
    fn CFRelease(cf: *const c_void);
    static kCFRunLoopCommonModes: CFStringRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementSetAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: CFTypeRef) -> i32;
    fn AXUIElementIsAttributeSettable(el: AXUIElementRef, attr: CFStringRef, settable: *mut u8) -> i32;
    fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, seconds: f32) -> i32;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    static AVMediaTypeAudio: &'static AnyObject;
}

// CGEventTap constants
const kCGHIDEventTap: u32 = 0;
const kCGSessionEventTap: u32 = 1;
const kCGHeadInsertEventTap: u32 = 0;
const kCGEventTapOptionDefault: u32 = 0;
const kCGEventKeyDown: u32 = 10;
const kCGEventKeyUp: u32 = 11;
const kCGEventFlagsChanged: u32 = 12;
const kCGEventTapDisabledByTimeout: u32 = 0xFFFF_FFFE;
const kCGEventTapDisabledByUserInput: u32 = 0xFFFF_FFFF;
const kCGKeyboardEventAutorepeat: u32 = 8;
const kCGKeyboardEventKeycode: u32 = 9;
const kCGEventSourceUserData: u32 = 42;
const kCGEventSourceStateHIDSystemState: i32 = 1;

// Device-independent modifier masks
const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_CONTROL: u64 = 0x0004_0000;
const FLAG_OPTION: u64 = 0x0008_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;
const FLAG_FN: u64 = 0x0080_0000;
const MODIFIERS: u64 = FLAG_SHIFT | FLAG_CONTROL | FLAG_OPTION | FLAG_COMMAND;
// Device-dependent (left/right) bits
const NX_LCTL: u64 = 0x0000_0001;
const NX_RCTL: u64 = 0x0000_2000;
const NX_LALT: u64 = 0x0000_0020;
const NX_RALT: u64 = 0x0000_0040;

// Virtual keycodes
const VK_RETURN: i64 = 36;
const VK_ESCAPE: i64 = 53;
const VK_KEYPAD_ENTER: i64 = 76;
const VK_V: u16 = 9;
const VK_LCTL: i64 = 59;
const VK_RCTL: i64 = 62;
const VK_LALT: i64 = 58;
const VK_RALT: i64 = 61;
const VK_FN: i64 = 63;
/// Every key that produces flagsChanged (modifiers + caps lock).
const MODIFIER_KEYCODES: [i64; 11] = [54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 179];

/// Marks events Mozhi posts so its own tap lets them through untouched. ("MOZH")
const MOZHI_TAG: i64 = 0x4D4F_5A48;

fn ax_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

// ── Key hook ────────────────────────────────────────────────────────────────

struct Tap {
    hotkey: Arc<RwLock<HotkeyConfig>>,
    capture: Arc<AtomicBool>,
    tx: Mutex<mpsc::Sender<KeyEvent>>,
    port: AtomicPtr<c_void>,
    state: Mutex<TapState>,
}

#[derive(Default)]
struct TapState {
    taps: DoubleTap,
    /// Keycodes whose keyDown we swallowed — swallow the matching keyUp too.
    swallowed: HashSet<i64>,
}

/// Keycodes for the trigger, with the left/right device bit that says "this one is down".
fn trigger_keys(key: TriggerKey) -> &'static [(i64, u64, u64)] {
    // (keycode, device bit, own device-independent mask)
    match key {
        TriggerKey::Control => &[(VK_LCTL, NX_LCTL, FLAG_CONTROL), (VK_RCTL, NX_RCTL, FLAG_CONTROL)],
        TriggerKey::RightControl => &[(VK_RCTL, NX_RCTL, FLAG_CONTROL)],
        TriggerKey::Option => &[(VK_LALT, NX_LALT, FLAG_OPTION), (VK_RALT, NX_RALT, FLAG_OPTION)],
        TriggerKey::RightOption => &[(VK_RALT, NX_RALT, FLAG_OPTION)],
        TriggerKey::Fn => &[(VK_FN, FLAG_FN, FLAG_FN)],
    }
}

extern "C" fn tap_callback(_proxy: *mut c_void, kind: u32, event: CGEventRef, info: *mut c_void) -> CGEventRef {
    // SAFETY: `info` is the leaked &'static Tap passed to CGEventTapCreate.
    let tap = unsafe { &*(info as *const Tap) };
    if kind == kCGEventTapDisabledByTimeout || kind == kCGEventTapDisabledByUserInput {
        let port = tap.port.load(Ordering::Acquire);
        if !port.is_null() {
            unsafe { CGEventTapEnable(port, true) };
        }
        return event;
    }
    if event.is_null() || unsafe { CGEventGetIntegerValueField(event, kCGEventSourceUserData) } == MOZHI_TAG {
        return event;
    }
    let keycode = unsafe { CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode) };
    let flags = unsafe { CGEventGetFlags(event) };
    let now = Instant::now();
    let mut st = tap.state.lock();

    match kind {
        kCGEventFlagsChanged => {
            let cfg = tap.hotkey.read().clone();
            if let Some(&(_, device_bit, own)) = trigger_keys(cfg.key).iter().find(|(k, _, _)| *k == keycode) {
                if flags & device_bit != 0 {
                    let others = flags & MODIFIERS & !own != 0;
                    st.taps.trigger_down(now, others, &cfg);
                } else if st.taps.trigger_up(now, &cfg) {
                    let _ = tap.tx.lock().send(KeyEvent::DoubleTap);
                }
            } else if MODIFIER_KEYCODES.contains(&keycode) {
                st.taps.other_input();
            }
        }
        kCGEventKeyDown => {
            st.taps.other_input();
            let plain = flags & MODIFIERS == 0;
            if plain && tap.capture.load(Ordering::SeqCst) && matches!(keycode, VK_RETURN | VK_KEYPAD_ENTER | VK_ESCAPE) {
                st.swallowed.insert(keycode);
                let repeat = unsafe { CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat) } != 0;
                if !repeat {
                    let ev = if keycode == VK_ESCAPE { KeyEvent::Escape } else { KeyEvent::Enter };
                    let _ = tap.tx.lock().send(ev);
                }
                return null_mut();
            }
        }
        kCGEventKeyUp if st.swallowed.remove(&keycode) => return null_mut(),
        _ => {}
    }
    event
}

pub fn start_key_listener(
    hotkey: Arc<RwLock<HotkeyConfig>>,
    capture: Arc<AtomicBool>,
    on_event: Box<dyn Fn(KeyEvent) + Send + Sync>,
) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel::<KeyEvent>();
    thread::Builder::new().name("mozhi-keys".into()).spawn(move || {
        for ev in rx {
            on_event(ev);
        }
    })?;

    let tap: &'static Tap = Box::leak(Box::new(Tap {
        hotkey,
        capture,
        tx: Mutex::new(tx),
        port: AtomicPtr::new(null_mut()),
        state: Mutex::new(TapState::default()),
    }));

    thread::Builder::new().name("mozhi-keytap".into()).spawn(move || {
        let mut warned = false;
        loop {
            if ax_trusted() {
                let mask = (1u64 << kCGEventKeyDown) | (1u64 << kCGEventKeyUp) | (1u64 << kCGEventFlagsChanged);
                let port = unsafe {
                    CGEventTapCreate(
                        kCGSessionEventTap,
                        kCGHeadInsertEventTap,
                        kCGEventTapOptionDefault,
                        mask,
                        tap_callback,
                        tap as *const Tap as *mut c_void,
                    )
                };
                if !port.is_null() {
                    tap.port.store(port, Ordering::Release);
                    unsafe {
                        let source = CFMachPortCreateRunLoopSource(null(), port, 0);
                        CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
                        CGEventTapEnable(port, true);
                    }
                    log::info!("keyboard hook active");
                    unsafe { CFRunLoopRun() };
                    log::warn!("keyboard hook run loop exited; restarting");
                }
            }
            if !warned {
                log::warn!("waiting for Accessibility permission to listen for the double-tap");
                warned = true;
            }
            thread::sleep(Duration::from_secs(2));
        }
    })?;
    Ok(())
}

// ── Objective-C helpers ─────────────────────────────────────────────────────

unsafe fn ns_to_string(obj: *mut AnyObject) -> Option<String> {
    if obj.is_null() {
        return None;
    }
    let p: *const c_char = msg_send![obj, UTF8String];
    if p.is_null() {
        return None;
    }
    Some(CStr::from_ptr(p).to_string_lossy().into_owned())
}

fn workspace_frontmost() -> *mut AnyObject {
    unsafe {
        let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        msg_send![ws, frontmostApplication]
    }
}

fn running_app(pid: i32) -> *mut AnyObject {
    unsafe { msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid] }
}

// ── Accessibility helpers ───────────────────────────────────────────────────

/// Owned AX element / CF value, released on drop.
struct Ax(CFType);

impl Ax {
    fn system_wide() -> Option<Ax> {
        Self::wrap(unsafe { AXUIElementCreateSystemWide() })
    }
    fn application(pid: i32) -> Option<Ax> {
        Self::wrap(unsafe { AXUIElementCreateApplication(pid) })
    }
    fn wrap(r: AXUIElementRef) -> Option<Ax> {
        if r.is_null() {
            return None;
        }
        unsafe { AXUIElementSetMessagingTimeout(r, 0.03) };
        Some(Ax(unsafe { CFType::wrap_under_create_rule(r) }))
    }
    fn ptr(&self) -> AXUIElementRef {
        self.0.as_CFTypeRef()
    }
    fn attr(&self, name: &str) -> Option<CFType> {
        let key = CFString::new(name);
        let mut out: CFTypeRef = null();
        let err = unsafe { AXUIElementCopyAttributeValue(self.ptr(), key.as_concrete_TypeRef(), &mut out) };
        if err != 0 || out.is_null() {
            return None;
        }
        Some(unsafe { CFType::wrap_under_create_rule(out) })
    }
    fn element(&self, name: &str) -> Option<Ax> {
        let v = self.attr(name)?;
        unsafe { AXUIElementSetMessagingTimeout(v.as_CFTypeRef(), 0.03) };
        Some(Ax(v))
    }
    fn string(&self, name: &str) -> Option<String> {
        let s = self.attr(name)?.downcast::<CFString>()?.to_string();
        let s = s.trim().to_string();
        (!s.is_empty()).then_some(s)
    }
    fn settable(&self, name: &str) -> bool {
        let key = CFString::new(name);
        let mut out: u8 = 0;
        unsafe { AXUIElementIsAttributeSettable(self.ptr(), key.as_concrete_TypeRef(), &mut out) == 0 && out != 0 }
    }
}

/// Chromium/Electron apps (Slack, VS Code, Discord…) only expose their AX tree when asked.
fn enable_manual_accessibility(app: &Ax, pid: i32) {
    static DONE: OnceLock<Mutex<HashSet<i32>>> = OnceLock::new();
    let done = DONE.get_or_init(Default::default);
    if done.lock().insert(pid) {
        let key = CFString::new("AXManualAccessibility");
        unsafe { AXUIElementSetAttributeValue(app.ptr(), key.as_concrete_TypeRef(), CFBoolean::true_value().as_CFTypeRef()) };
    }
}

// ── Frontmost app ───────────────────────────────────────────────────────────

pub fn frontmost_app() -> Option<FrontApp> {
    autoreleasepool(|_| unsafe {
        let app = workspace_frontmost();
        if app.is_null() {
            return None;
        }
        let pid: i32 = msg_send![app, processIdentifier];
        let name = ns_to_string(msg_send![app, localizedName]).unwrap_or_else(|| "Unknown app".into());
        let id = ns_to_string(msg_send![app, bundleIdentifier]).unwrap_or_else(|| format!("pid:{pid}"));
        let window_title = if ax_trusted() {
            Ax::application(pid).and_then(|a| a.element("AXFocusedWindow")).and_then(|w| w.string("AXTitle"))
        } else {
            None
        };
        Some(FrontApp { name, id, pid, window_title, icon: None, is_self: pid == std::process::id() as i32 })
    })
}

pub fn app_icon(app: &FrontApp) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().get(&app.id) {
        return hit.clone();
    }
    let icon = autoreleasepool(|_| unsafe { render_icon(app.pid, 64) });
    cache.lock().insert(app.id.clone(), icon.clone());
    icon
}

unsafe fn render_icon(pid: i32, px: usize) -> Option<String> {
    let app = running_app(pid);
    if app.is_null() {
        return None;
    }
    let image: *mut AnyObject = msg_send![app, icon];
    if image.is_null() {
        return None;
    }
    let mut proposed = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(px as f64 * 2.0, px as f64 * 2.0));
    let src: *mut c_void =
        msg_send![image, CGImageForProposedRect: &mut proposed as *mut NSRect, context: null_mut::<AnyObject>(), hints: null_mut::<AnyObject>()];
    if src.is_null() {
        return None;
    }
    // Downscale into an exact px×px RGBA bitmap.
    let cs = CGColorSpaceCreateDeviceRGB();
    let ctx = CGBitmapContextCreate(null_mut(), px, px, 8, 0, cs, 1 /* premultiplied last */);
    CGColorSpaceRelease(cs);
    if ctx.is_null() {
        return None;
    }
    CGContextSetInterpolationQuality(ctx, 3 /* high */);
    let rect = CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { width: px as f64, height: px as f64 } };
    CGContextDrawImage(ctx, rect, src);
    let scaled = CGBitmapContextCreateImage(ctx);
    CGContextRelease(ctx);
    if scaled.is_null() {
        return None;
    }
    let rep: *mut AnyObject = msg_send![class!(NSBitmapImageRep), alloc];
    let rep: *mut AnyObject = msg_send![rep, initWithCGImage: scaled];
    CGImageRelease(scaled);
    if rep.is_null() {
        return None;
    }
    let props: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
    let png: *mut AnyObject = msg_send![rep, representationUsingType: 4usize /* PNG */, properties: props];
    let out = if png.is_null() {
        None
    } else {
        let len: usize = msg_send![png, length];
        let bytes: *const c_void = msg_send![png, bytes];
        let slice = std::slice::from_raw_parts(bytes as *const u8, len);
        Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(slice)))
    };
    let _: () = msg_send![rep, release];
    out
}

// ── Focused element ─────────────────────────────────────────────────────────

pub fn focused_element() -> Option<FocusedElement> {
    if !ax_trusted() {
        return None;
    }
    let focused = Ax::system_wide().and_then(|s| s.element("AXFocusedUIElement")).or_else(|| {
        // Electron apps answer only after AXManualAccessibility is set on the app.
        let pid: i32 = autoreleasepool(|_| unsafe {
            let app = workspace_frontmost();
            if app.is_null() {
                0
            } else {
                msg_send![app, processIdentifier]
            }
        });
        let app = Ax::application(pid)?;
        enable_manual_accessibility(&app, pid);
        app.element("AXFocusedUIElement")
    })?;

    let role = focused.string("AXRole").unwrap_or_default();
    let subrole = focused.string("AXSubrole").unwrap_or_default();
    let label = ["AXPlaceholderValue", "AXDescription", "AXTitle", "AXHelp"]
        .iter()
        .find_map(|a| focused.string(a))
        .or_else(|| focused.element("AXTitleUIElement").and_then(|t| t.string("AXValue")))
        .map(|s| truncate(&s, 80));

    let normalised = match (role.as_str(), subrole.as_str()) {
        ("AXTextField", "AXSearchField") => "searchField",
        ("AXTextField", _) => "textField",
        ("AXTextArea", _) => "textArea",
        ("AXComboBox", _) => "comboBox",
        ("AXWebArea", _) => "webArea",
        _ => "other",
    };
    let editable = matches!(normalised, "textField" | "textArea" | "searchField" | "comboBox")
        || focused.settable("AXValue")
        || focused.attr("AXEditableAncestor").is_some();

    Some(FocusedElement { role: normalised.into(), label, editable })
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        s
    } else {
        s.chars().take(max - 1).collect::<String>() + "…"
    }
}

// ── Clipboard & insertion ───────────────────────────────────────────────────

const PB_STRING: &str = "public.utf8-plain-text";
/// nspasteboard.org convention: clipboard managers skip transient entries.
const PB_TRANSIENT: &str = "org.nspasteboard.TransientType";

type PasteboardSnapshot = Vec<Vec<(String, Vec<u8>)>>;

fn pasteboard() -> *mut AnyObject {
    unsafe { msg_send![class!(NSPasteboard), generalPasteboard] }
}

fn change_count() -> isize {
    unsafe { msg_send![pasteboard(), changeCount] }
}

unsafe fn nsdata_bytes(data: *mut AnyObject) -> Vec<u8> {
    if data.is_null() {
        return Vec::new();
    }
    let len: usize = msg_send![data, length];
    let bytes: *const c_void = msg_send![data, bytes];
    if bytes.is_null() || len == 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts(bytes as *const u8, len).to_vec()
}

unsafe fn nsdata(bytes: &[u8]) -> *mut AnyObject {
    msg_send![class!(NSData), dataWithBytes: bytes.as_ptr() as *const c_void, length: bytes.len()]
}

fn snapshot_pasteboard() -> PasteboardSnapshot {
    autoreleasepool(|_| unsafe {
        let mut out = Vec::new();
        let items: *mut AnyObject = msg_send![pasteboard(), pasteboardItems];
        if items.is_null() {
            return out;
        }
        let n: usize = msg_send![items, count];
        for i in 0..n {
            let item: *mut AnyObject = msg_send![items, objectAtIndex: i];
            let types: *mut AnyObject = msg_send![item, types];
            let tn: usize = msg_send![types, count];
            let mut entries = Vec::new();
            for j in 0..tn {
                let t: *mut AnyObject = msg_send![types, objectAtIndex: j];
                let data: *mut AnyObject = msg_send![item, dataForType: t];
                if let Some(name) = ns_to_string(t) {
                    entries.push((name, nsdata_bytes(data)));
                }
            }
            out.push(entries);
        }
        out
    })
}

fn restore_pasteboard(snapshot: &PasteboardSnapshot) {
    autoreleasepool(|_| unsafe {
        let pb = pasteboard();
        let _: isize = msg_send![pb, clearContents];
        if snapshot.is_empty() {
            return;
        }
        let array: *mut AnyObject = msg_send![class!(NSMutableArray), array];
        for entries in snapshot {
            let item: *mut AnyObject = msg_send![class!(NSPasteboardItem), alloc];
            let item: *mut AnyObject = msg_send![item, init];
            for (t, bytes) in entries {
                let ty = NSString::from_str(t);
                let _: bool = msg_send![item, setData: nsdata(bytes), forType: &*ty];
            }
            let _: () = msg_send![array, addObject: item];
            let _: () = msg_send![item, release];
        }
        let _: bool = msg_send![pb, writeObjects: array];
    })
}

fn write_string(text: &str, transient: bool) -> isize {
    autoreleasepool(|_| unsafe {
        let pb = pasteboard();
        let _: isize = msg_send![pb, clearContents];
        let s = NSString::from_str(text);
        let ty = NSString::from_str(PB_STRING);
        let _: bool = msg_send![pb, setString: &*s, forType: &*ty];
        if transient {
            let tt = NSString::from_str(PB_TRANSIENT);
            let _: bool = msg_send![pb, setData: nsdata(&[]), forType: &*tt];
        }
        msg_send![pb, changeCount]
    })
}

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    write_string(text, false);
    Ok(())
}

fn frontmost_pid() -> i32 {
    autoreleasepool(|_| unsafe {
        let app = workspace_frontmost();
        if app.is_null() {
            0
        } else {
            msg_send![app, processIdentifier]
        }
    })
}

fn activate(pid: i32) -> bool {
    autoreleasepool(|_| unsafe {
        let app = running_app(pid);
        if app.is_null() {
            return false;
        }
        // NSApplicationActivateIgnoringOtherApps
        msg_send![app, activateWithOptions: 2usize]
    })
}

fn post_paste() {
    unsafe {
        let src = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
        for down in [true, false] {
            let ev = CGEventCreateKeyboardEvent(src, VK_V, down);
            if ev.is_null() {
                continue;
            }
            // Command only: a still-held Control must not turn this into ⌃⌘V.
            CGEventSetFlags(ev, FLAG_COMMAND);
            CGEventSetIntegerValueField(ev, kCGEventSourceUserData, MOZHI_TAG);
            CGEventPost(kCGHIDEventTap, ev);
            CFRelease(ev);
            thread::sleep(Duration::from_millis(8));
        }
        if !src.is_null() {
            CFRelease(src);
        }
    }
}

pub fn insert_text(text: &str, target: &FrontApp, opts: InsertOptions) -> anyhow::Result<()> {
    if !ax_trusted() {
        anyhow::bail!("Accessibility permission is required to paste");
    }
    let snapshot = opts.restore_clipboard.then(snapshot_pasteboard);

    if frontmost_pid() != target.pid {
        if !activate(target.pid) {
            anyhow::bail!("{} is no longer running", target.name);
        }
        let deadline = Instant::now() + Duration::from_millis(300);
        while frontmost_pid() != target.pid && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(15));
        }
        thread::sleep(Duration::from_millis(40)); // let the app restore its first responder
    }

    let ours = write_string(text, opts.restore_clipboard);
    post_paste();

    if let Some(snapshot) = snapshot {
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(350));
            // Only restore if nobody (the user, another app) has written since.
            if change_count() == ours {
                restore_pasteboard(&snapshot);
            }
        });
    }
    Ok(())
}

// ── Permissions ─────────────────────────────────────────────────────────────

fn mic_state() -> PermState {
    autoreleasepool(|_| unsafe {
        let cls = AnyClass::get(c"AVCaptureDevice");
        let Some(cls) = cls else { return PermState::NotDetermined };
        let status: isize = msg_send![cls, authorizationStatusForMediaType: AVMediaTypeAudio];
        match status {
            3 => PermState::Granted,
            1 | 2 => PermState::Denied,
            _ => PermState::NotDetermined,
        }
    })
}

pub fn permissions() -> Permissions {
    Permissions {
        microphone: mic_state(),
        accessibility: if ax_trusted() { PermState::Granted } else { PermState::Denied },
    }
}

pub fn request_permission(kind: PermissionKind) {
    match kind {
        PermissionKind::Accessibility => unsafe {
            let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
            let opts = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
            AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef() as *const c_void);
        },
        PermissionKind::Microphone => {
            if mic_state() != PermState::NotDetermined {
                open_permission_settings(kind);
                return;
            }
            autoreleasepool(|_| unsafe {
                if let Some(cls) = AnyClass::get(c"AVCaptureDevice") {
                    let block = block2::RcBlock::new(|granted: Bool| log::info!("microphone access granted: {}", granted.as_bool()));
                    let _: () = msg_send![cls, requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*block];
                }
            })
        }
    }
}

pub fn open_permission_settings(kind: PermissionKind) {
    let pane = match kind {
        PermissionKind::Accessibility => "Privacy_Accessibility",
        PermissionKind::Microphone => "Privacy_Microphone",
    };
    let url = format!("x-apple.systempreferences:com.apple.preference.security?{pane}");
    if let Err(e) = std::process::Command::new("open").arg(url).spawn() {
        log::warn!("open settings: {e}");
    }
}

// ── Overlay panel ───────────────────────────────────────────────────────────

const NS_WINDOW_STYLE_NONACTIVATING_PANEL: usize = 1 << 7;
const NS_STATUS_WINDOW_LEVEL: isize = 25;
// canJoinAllSpaces | stationary | ignoresCycle | fullScreenAuxiliary
const OVERLAY_COLLECTION: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
const BOTTOM_MARGIN: f64 = 72.0;

extern "C" fn no(_this: &AnyObject, _cmd: Sel) -> Bool {
    Bool::NO
}

/// True only while the user edits the transcript in the overlay.
static OVERLAY_KEYABLE: AtomicBool = AtomicBool::new(false);

extern "C" fn keyable(_this: &AnyObject, _cmd: Sel) -> Bool {
    Bool::new(OVERLAY_KEYABLE.load(Ordering::SeqCst))
}

/// NSPanel subclass that never becomes main, and becomes key only while the transcript is
/// being edited. Being a non-activating panel, even then Mozhi is not activated: the user's
/// app stays frontmost and gets keyboard focus back the moment the panel lets go.
/// NSPanel declares no ivars of its own, so re-classing tao's NSWindow subclass instance
/// onto it is layout-compatible.
fn panel_class() -> &'static AnyClass {
    static CLASS: OnceLock<usize> = OnceLock::new();
    let ptr = *CLASS.get_or_init(|| {
        let superclass = AnyClass::get(c"NSPanel").expect("NSPanel");
        let cls = match ClassBuilder::new(c"MozhiOverlayPanel", superclass) {
            Some(mut b) => unsafe {
                b.add_method(sel!(canBecomeKeyWindow), keyable as extern "C" fn(_, _) -> _);
                b.add_method(sel!(canBecomeMainWindow), no as extern "C" fn(_, _) -> _);
                b.register()
            },
            None => AnyClass::get(c"MozhiOverlayPanel").expect("MozhiOverlayPanel"),
        };
        cls as *const AnyClass as usize
    });
    unsafe { &*(ptr as *const AnyClass) }
}

/// Raw pointers are not Send; carry the NSWindow address across to the main thread.
#[derive(Clone, Copy)]
struct WindowPtr(usize);

impl WindowPtr {
    fn of(window: &tauri::WebviewWindow) -> anyhow::Result<Self> {
        Ok(Self(window.ns_window()? as usize))
    }
    fn get(self) -> *mut AnyObject {
        self.0 as *mut AnyObject
    }
}

pub fn setup_overlay(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    let w = WindowPtr::of(window)?;
    window.run_on_main_thread(move || unsafe {
        let win = w.get();
        if win.is_null() {
            return;
        }
        AnyObject::set_class(&*win, panel_class());
        let mask: usize = msg_send![win, styleMask];
        let _: () = msg_send![win, setStyleMask: mask | NS_WINDOW_STYLE_NONACTIVATING_PANEL];
        let _: () = msg_send![win, setFloatingPanel: true];
        let _: () = msg_send![win, setBecomesKeyOnlyIfNeeded: true];
        let _: () = msg_send![win, setHidesOnDeactivate: false];
        let _: () = msg_send![win, setWorksWhenModal: true];
        let _: () = msg_send![win, setLevel: NS_STATUS_WINDOW_LEVEL];
        let _: () = msg_send![win, setCollectionBehavior: OVERLAY_COLLECTION];
        let _: () = msg_send![win, setHasShadow: false];
    })?;
    Ok(())
}

pub fn show_overlay(window: &tauri::WebviewWindow, position: Option<(f64, f64)>) -> anyhow::Result<()> {
    let w = WindowPtr::of(window)?;
    let win_handle = window.clone();
    window.run_on_main_thread(move || unsafe {
        let win = w.get();
        if win.is_null() {
            return;
        }
        match position.filter(|&(x, y)| on_some_screen(x, y)) {
            // Saved spot: Tauri converts its top-left logical coordinates for us.
            Some((x, y)) => {
                let _ = win_handle.set_position(tauri::LogicalPosition::new(x, y));
            }
            None => {
                let frame: NSRect = msg_send![win, frame];
                let area = screen_under_cursor_visible_frame();
                let origin = NSPoint::new(
                    (area.origin.x + (area.size.width - frame.size.width) / 2.0).round(),
                    (area.origin.y + BOTTOM_MARGIN).round(),
                );
                let _: () = msg_send![win, setFrameOrigin: origin];
            }
        }
        let _: () = msg_send![win, orderFrontRegardless];
    })?;
    Ok(())
}

pub fn hide_overlay(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    OVERLAY_KEYABLE.store(false, Ordering::SeqCst);
    let w = WindowPtr::of(window)?;
    window.run_on_main_thread(move || unsafe {
        let win = w.get();
        if !win.is_null() {
            let _: () = msg_send![win, orderOut: null_mut::<AnyObject>()];
        }
    })?;
    Ok(())
}

/// Runs `f` on the main thread and waits for it (≤ 1 s). Never call from the main thread.
fn on_main_sync(window: &tauri::WebviewWindow, f: impl FnOnce() + Send + 'static) -> anyhow::Result<()> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    window.run_on_main_thread(move || {
        f();
        let _ = tx.send(());
    })?;
    let _ = rx.recv_timeout(Duration::from_secs(1));
    Ok(())
}

pub fn focus_overlay(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    OVERLAY_KEYABLE.store(true, Ordering::SeqCst);
    let w = WindowPtr::of(window)?;
    window.run_on_main_thread(move || unsafe {
        let win = w.get();
        if !win.is_null() {
            let _: () = msg_send![win, makeKeyWindow];
        }
    })?;
    Ok(())
}

pub fn unfocus_overlay(window: &tauri::WebviewWindow, target: Option<&FrontApp>) -> anyhow::Result<()> {
    if !OVERLAY_KEYABLE.swap(false, Ordering::SeqCst) {
        return Ok(());
    }
    let w = WindowPtr::of(window)?;
    // Ordering the panel out drops its key status; bring it straight back as a non-key panel.
    on_main_sync(window, move || unsafe {
        let win = w.get();
        if !win.is_null() {
            let _: () = msg_send![win, orderOut: null_mut::<AnyObject>()];
            let _: () = msg_send![win, orderFrontRegardless];
        }
    })?;
    // The target never stopped being the active app, but re-activating it makes its window
    // key again so the paste lands in the field the user was typing in.
    if let Some(t) = target {
        activate(t.pid);
        thread::sleep(Duration::from_millis(60));
    }
    Ok(())
}

/// Visible frame (minus menu bar and Dock) of the screen under the mouse, in Cocoa coordinates.
unsafe fn screen_under_cursor_visible_frame() -> NSRect {
    let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];
    let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
    let n: usize = if screens.is_null() { 0 } else { msg_send![screens, count] };
    for i in 0..n {
        let s: *mut AnyObject = msg_send![screens, objectAtIndex: i];
        let f: NSRect = msg_send![s, frame];
        if mouse.x >= f.origin.x && mouse.x < f.origin.x + f.size.width && mouse.y >= f.origin.y && mouse.y < f.origin.y + f.size.height {
            return msg_send![s, visibleFrame];
        }
    }
    let main: *mut AnyObject = msg_send![class!(NSScreen), mainScreen];
    if main.is_null() {
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0))
    } else {
        msg_send![main, visibleFrame]
    }
}

/// Is a saved top-left (Tauri logical coords: origin at the primary screen's top-left,
/// y down) still on a connected screen? Monitors get unplugged.
unsafe fn on_some_screen(x: f64, y: f64) -> bool {
    let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
    let n: usize = if screens.is_null() { 0 } else { msg_send![screens, count] };
    if n == 0 {
        return false;
    }
    let primary: *mut AnyObject = msg_send![screens, objectAtIndex: 0usize];
    let pf: NSRect = msg_send![primary, frame];
    let cy = pf.size.height - y; // flip to Cocoa (origin bottom-left of primary)
    (0..n).any(|i| {
        let s: *mut AnyObject = msg_send![screens, objectAtIndex: i];
        let f: NSRect = msg_send![s, frame];
        x >= f.origin.x - 20.0 && x < f.origin.x + f.size.width - 40.0 && cy > f.origin.y + 40.0 && cy <= f.origin.y + f.size.height + 20.0
    })
}

pub fn set_dock_visible(app: &tauri::AppHandle, visible: bool) {
    let policy = if visible { tauri::ActivationPolicy::Regular } else { tauri::ActivationPolicy::Accessory };
    if let Err(e) = app.set_activation_policy(policy) {
        log::warn!("activation policy: {e}");
    }
}

#[cfg(test)]
mod tests {
    //! Drives the real tap callback with synthetic CGEvents (creating events needs no permission).
    use super::*;

    extern "C" {
        fn CGEventSetType(event: CGEventRef, kind: u32);
    }

    struct Harness {
        tap: &'static Tap,
        rx: mpsc::Receiver<KeyEvent>,
    }

    impl Harness {
        fn new(capture: bool) -> Self {
            let (tx, rx) = mpsc::channel();
            let tap = Box::leak(Box::new(Tap {
                hotkey: Arc::new(RwLock::new(HotkeyConfig::default())),
                capture: Arc::new(AtomicBool::new(capture)),
                tx: Mutex::new(tx),
                port: AtomicPtr::new(null_mut()),
                state: Mutex::new(TapState::default()),
            }));
            Self { tap, rx }
        }

        /// Feeds one event; returns true if it passed through (was not swallowed).
        fn feed(&self, kind: u32, keycode: u16, flags: u64) -> bool {
            unsafe {
                let ev = CGEventCreateKeyboardEvent(null_mut(), keycode, kind != kCGEventKeyUp);
                CGEventSetType(ev, kind);
                CGEventSetFlags(ev, flags);
                let out = tap_callback(null_mut(), kind, ev, self.tap as *const Tap as *mut c_void);
                CFRelease(ev);
                !out.is_null()
            }
        }

        fn ctrl(&self, down: bool) {
            let flags = if down { FLAG_CONTROL | NX_LCTL } else { 0 };
            assert!(self.feed(kCGEventFlagsChanged, VK_LCTL as u16, flags), "modifier events always pass through");
        }

        fn events(&self) -> Vec<KeyEvent> {
            self.rx.try_iter().collect()
        }
    }

    #[test]
    fn double_tap_control_is_reported() {
        let h = Harness::new(false);
        h.ctrl(true);
        h.ctrl(false);
        h.ctrl(true);
        h.ctrl(false);
        assert_eq!(h.events(), vec![KeyEvent::DoubleTap]);
    }

    #[test]
    fn right_control_counts_for_either_control() {
        let h = Harness::new(false);
        for _ in 0..2 {
            h.feed(kCGEventFlagsChanged, VK_RCTL as u16, FLAG_CONTROL | NX_RCTL);
            h.feed(kCGEventFlagsChanged, VK_RCTL as u16, 0);
        }
        assert_eq!(h.events(), vec![KeyEvent::DoubleTap]);
    }

    #[test]
    fn ctrl_c_then_tap_is_not_a_double_tap() {
        let h = Harness::new(false);
        h.ctrl(true);
        assert!(h.feed(kCGEventKeyDown, 8 /* C */, FLAG_CONTROL | NX_LCTL));
        h.feed(kCGEventKeyUp, 8, FLAG_CONTROL | NX_LCTL);
        h.ctrl(false);
        h.ctrl(true);
        h.ctrl(false);
        assert!(h.events().is_empty());
    }

    #[test]
    fn shift_held_blocks_the_trigger() {
        let h = Harness::new(false);
        for _ in 0..2 {
            h.feed(kCGEventFlagsChanged, VK_LCTL as u16, FLAG_CONTROL | NX_LCTL | FLAG_SHIFT);
            h.feed(kCGEventFlagsChanged, VK_LCTL as u16, FLAG_SHIFT);
        }
        assert!(h.events().is_empty());
    }

    #[test]
    fn enter_and_escape_pass_through_when_not_capturing() {
        let h = Harness::new(false);
        assert!(h.feed(kCGEventKeyDown, VK_RETURN as u16, 0));
        assert!(h.feed(kCGEventKeyUp, VK_RETURN as u16, 0));
        assert!(h.feed(kCGEventKeyDown, VK_ESCAPE as u16, 0));
        assert!(h.events().is_empty());
    }

    #[test]
    fn enter_and_escape_are_swallowed_while_capturing() {
        let h = Harness::new(true);
        assert!(!h.feed(kCGEventKeyDown, VK_RETURN as u16, 0));
        assert!(!h.feed(kCGEventKeyUp, VK_RETURN as u16, 0));
        assert!(!h.feed(kCGEventKeyDown, VK_KEYPAD_ENTER as u16, 0));
        assert!(!h.feed(kCGEventKeyUp, VK_KEYPAD_ENTER as u16, 0));
        assert!(!h.feed(kCGEventKeyDown, VK_ESCAPE as u16, 0));
        // Capture ends between down and up: the orphan key-up is still swallowed.
        h.tap.capture.store(false, Ordering::SeqCst);
        assert!(!h.feed(kCGEventKeyUp, VK_ESCAPE as u16, 0));
        assert_eq!(h.events(), vec![KeyEvent::Enter, KeyEvent::Enter, KeyEvent::Escape]);
    }

    #[test]
    fn shift_enter_is_not_captured() {
        let h = Harness::new(true);
        assert!(h.feed(kCGEventKeyDown, VK_RETURN as u16, FLAG_SHIFT));
        assert!(h.events().is_empty());
    }

    #[test]
    fn mozhi_posted_events_are_ignored() {
        let h = Harness::new(true);
        unsafe {
            let ev = CGEventCreateKeyboardEvent(null_mut(), VK_RETURN as u16, true);
            CGEventSetIntegerValueField(ev, kCGEventSourceUserData, MOZHI_TAG);
            let out = tap_callback(null_mut(), kCGEventKeyDown, ev, h.tap as *const Tap as *mut c_void);
            assert!(!out.is_null());
            CFRelease(ev);
        }
        assert!(h.events().is_empty());
    }

    #[test]
    fn frontmost_app_and_icon() {
        let app = frontmost_app().expect("some app is frontmost");
        assert!(!app.name.is_empty());
        let icon = app_icon(&app).expect("icon");
        assert!(icon.starts_with("data:image/png;base64,"));
    }
}
