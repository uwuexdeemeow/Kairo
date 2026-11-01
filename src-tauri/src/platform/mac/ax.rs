//! The address Firefox and Opera show on a Mac, which they don't tell through Apple Events, read
//! from their address bar through the Accessibility API, as VoiceOver would. macOS asks once
//! whether Kairo may (Privacy & Security → Accessibility).

use std::collections::HashMap;
use std::ffi::c_void;

use super::{string, CFArrayGetCount, CFArrayGetValueAtIndex, CFRelease, CFTypeRef};
use crate::platform::Address;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFBooleanTrue: CFTypeRef;
    static kCFBooleanFalse: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(boolean: CFTypeRef) -> bool;
    fn CFStringCreateWithCString(allocator: CFTypeRef, text: *const std::ffi::c_char, encoding: u32) -> CFTypeRef;
    fn CFDictionaryCreate(
        allocator: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFTypeRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFTypeRef;
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(element: CFTypeRef, attribute: CFTypeRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> i32;
    fn AXUIElementSetAttributeValue(element: CFTypeRef, attribute: CFTypeRef, value: CFTypeRef) -> i32;
    fn AXUIElementPerformAction(element: CFTypeRef, action: CFTypeRef) -> i32;
    /// The Core Graphics number of an accessibility window. Private, but what window switchers
    /// on the Mac rely on (alt-tab-macos, Hammerspoon).
    fn _AXUIElementGetWindow(element: CFTypeRef, number: *mut u32) -> i32;
}

const UTF8: u32 = 0x0800_0100;
/// A browser that's slow to answer is given up on for this second rather than holding Kairo up.
const TIMEOUT_SECONDS: f32 = 0.25;
/// How many elements to look through for the address bar; it comes early, before the page.
const SEARCH_LIMIT: usize = 3000;

/// A Core Foundation object Kairo holds a reference to.
struct Owned(CFTypeRef);

impl Owned {
    /// Takes an object a Copy or Create call returned; None for null.
    fn new(cf: CFTypeRef) -> Option<Self> {
        (!cf.is_null()).then_some(Self(cf))
    }

    /// Keeps an object something else holds, like an array's element.
    fn retain(cf: CFTypeRef) -> Option<Self> {
        Self::new(if cf.is_null() { cf } else { unsafe { CFRetain(cf) } })
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

fn cf_string(text: &str) -> Owned {
    let text = std::ffi::CString::new(text).expect("attribute names have no nulls");
    Owned::new(unsafe { CFStringCreateWithCString(std::ptr::null(), text.as_ptr(), UTF8) }).expect("CFString")
}

/// An attribute of an accessibility element.
fn attribute(element: CFTypeRef, name: &str) -> Option<Owned> {
    let name = cf_string(name);
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe { AXUIElementCopyAttributeValue(element, name.0, &mut value) };
    if error == 0 {
        Owned::new(value)
    } else {
        None
    }
}

fn text_attribute(element: CFTypeRef, name: &str) -> Option<String> {
    let value = attribute(element, name)?;
    unsafe { string(value.0) }
}

fn bool_attribute(element: CFTypeRef, name: &str) -> Option<bool> {
    let value = attribute(element, name)?;
    unsafe { (CFGetTypeID(value.0) == CFBooleanGetTypeID()).then(|| CFBooleanGetValue(value.0)) }
}

/// Whether macOS lets Kairo act on other apps' windows, without asking.
pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// App `pid`'s window with this Core Graphics number, or else the first with this title, so two
/// windows with one title aren't mixed up where the number can be read.
fn window_titled(pid: i32, number: u32, title: &str) -> Option<Owned> {
    let app = Owned::new(unsafe { AXUIElementCreateApplication(pid) })?;
    unsafe { AXUIElementSetMessagingTimeout(app.0, TIMEOUT_SECONDS) };
    let windows = attribute(app.0, "AXWindows")?;
    unsafe {
        if CFGetTypeID(windows.0) != CFArrayGetTypeID() {
            return None;
        }
        let all: Vec<Owned> = (0..CFArrayGetCount(windows.0)).filter_map(|i| Owned::retain(CFArrayGetValueAtIndex(windows.0, i))).collect();
        let numbered = |w: &Owned| {
            let mut n = 0u32;
            _AXUIElementGetWindow(w.0, &mut n) == 0 && n == number
        };
        let at = all.iter().position(numbered).or_else(|| all.iter().position(|w| text_attribute(w.0, "AXTitle").as_deref() == Some(title)))?;
        all.into_iter().nth(at)
    }
}

fn perform(element: CFTypeRef, action: &str) -> bool {
    let action = cf_string(action);
    unsafe { AXUIElementPerformAction(element, action.0) == 0 }
}

/// Brings one window of app `pid` to the front of its app, out of the Dock if minimized.
pub fn raise(pid: i32, number: u32, title: &str) -> bool {
    let Some(window) = window_titled(pid, number, title) else { return false };
    if bool_attribute(window.0, "AXMinimized") == Some(true) {
        let minimized = cf_string("AXMinimized");
        unsafe { AXUIElementSetAttributeValue(window.0, minimized.0, kCFBooleanFalse) };
    }
    perform(window.0, "AXRaise")
}

/// Presses the close button of one window of app `pid`, so the app can still ask about unsaved
/// work.
pub fn close(pid: i32, number: u32, title: &str) -> bool {
    let Some(window) = window_titled(pid, number, title) else { return false };
    attribute(window.0, "AXCloseButton").is_some_and(|button| perform(button.0, "AXPress"))
}

/// Address bars found so far, by app.
pub struct AddressBars {
    found: HashMap<i32, Owned>,
    asked: bool,
}

impl AddressBars {
    pub fn new() -> Self {
        Self { found: HashMap::new(), asked: false }
    }

    /// What the address bar of app `pid`'s front window shows; None when Kairo may not look or
    /// hasn't found it.
    pub fn read(&mut self, pid: i32, firefox: bool) -> Option<Address> {
        if !self.allowed() {
            return None;
        }
        if !self.found.contains_key(&pid) {
            let bar = find(pid, firefox)?;
            self.found.insert(pid, bar);
        }
        let bar = self.found.get(&pid)?.0;
        if bool_attribute(bar, "AXFocused") == Some(true) {
            return Some(Address::Typing);
        }
        match text_attribute(bar, "AXValue") {
            Some(text) => Some(Address::Shows(text)),
            // The window closed, or the bar went with a tab; it's found afresh.
            None => {
                self.found.remove(&pid);
                None
            }
        }
    }

    /// Whether macOS lets Kairo read other apps; the first time, macOS asks.
    fn allowed(&mut self) -> bool {
        if std::mem::replace(&mut self.asked, true) {
            return unsafe { AXIsProcessTrusted() };
        }
        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let Some(options) = Owned::new(CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            )) else {
                return AXIsProcessTrusted();
            };
            AXIsProcessTrustedWithOptions(options.0)
        }
    }
}

/// The address bar in app `pid`'s front window, in the window's own order and outside the page,
/// whose text boxes aren't it: Firefox's carries its id, and a Chromium browser's is its first
/// text field.
fn find(pid: i32, firefox: bool) -> Option<Owned> {
    let app = Owned::new(unsafe { AXUIElementCreateApplication(pid) })?;
    unsafe { AXUIElementSetMessagingTimeout(app.0, TIMEOUT_SECONDS) };
    let window = attribute(app.0, "AXFocusedWindow")?;
    let mut stack = vec![window];
    let mut seen = 0;
    while let Some(element) = stack.pop() {
        seen += 1;
        if seen > SEARCH_LIMIT {
            return None;
        }
        let role = text_attribute(element.0, "AXRole").unwrap_or_default();
        if role == "AXWebArea" {
            continue;
        }
        if matches!(role.as_str(), "AXTextField" | "AXComboBox")
            && (!firefox || text_attribute(element.0, "AXDOMIdentifier").as_deref() == Some("urlbar-input"))
        {
            return Some(element);
        }
        let Some(children) = attribute(element.0, "AXChildren") else { continue };
        unsafe {
            if CFGetTypeID(children.0) != CFArrayGetTypeID() {
                continue;
            }
            // Last pushed comes off first, so the first child is looked at next.
            for i in (0..CFArrayGetCount(children.0)).rev() {
                stack.extend(Owned::retain(CFArrayGetValueAtIndex(children.0, i)));
            }
        }
    }
    None
}
