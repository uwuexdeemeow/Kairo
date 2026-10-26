//! The address a browser shows on Windows, read from its address bar through UI Automation, as a
//! screen reader would, so sites are exact without the browser extension.

use std::collections::HashMap;

use windows::core::{Interface, BSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationElement, IUIAutomationValuePattern, TreeScope_Descendants,
    UIA_AutomationIdPropertyId, UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::IsWindow;

pub enum Address {
    /// The text in the address bar, usually the page's address without "https://".
    Shows(String),
    /// Someone is typing in it, so it shows what they type rather than the page.
    Typing,
}

/// Address bars found so far, by window; finding one takes a moment, reading it doesn't.
/// COM objects stay on the thread that made them.
pub struct AddressBars {
    automation: IUIAutomation,
    /// Chromium browsers' address bar is the first text box in the window.
    text_box: IUIAutomationCondition,
    /// Firefox names its address bar.
    firefox_bar: IUIAutomationCondition,
    found: HashMap<usize, IUIAutomationElement>,
}

impl AddressBars {
    pub fn new() -> Option<Self> {
        unsafe {
            // Fails harmlessly when the thread already has COM.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let text_box = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_EditControlTypeId.0)).ok()?;
            let firefox_bar = automation.CreatePropertyCondition(UIA_AutomationIdPropertyId, &VARIANT::from(BSTR::from("urlbar-input"))).ok()?;
            Some(Self { automation, text_box, firefox_bar, found: HashMap::new() })
        }
    }

    /// What the address bar of the browser window `window` shows; None when it has none.
    pub fn read(&mut self, window: usize, firefox: bool) -> Option<Address> {
        let bar = match self.found.get(&window) {
            Some(bar) => bar.clone(),
            None => {
                let bar = self.find(window, firefox)?;
                // Closed windows go when a new one comes, so the list doesn't grow.
                self.found.retain(|&w, _| unsafe { IsWindow(Some(HWND(w as *mut _))).as_bool() });
                self.found.insert(window, bar.clone());
                bar
            }
        };
        let read = || -> windows::core::Result<Address> {
            unsafe {
                if bar.CurrentHasKeyboardFocus()?.as_bool() {
                    return Ok(Address::Typing);
                }
                let value: IUIAutomationValuePattern = bar.GetCurrentPattern(UIA_ValuePatternId)?.cast()?;
                Ok(Address::Shows(value.CurrentValue()?.to_string()))
            }
        };
        match read() {
            Ok(address) => Some(address),
            // The window closed, or the bar went with a tab dragged out; it's found afresh.
            Err(_) => {
                self.found.remove(&window);
                None
            }
        }
    }

    fn find(&self, window: usize, firefox: bool) -> Option<IUIAutomationElement> {
        unsafe {
            let root = self.automation.ElementFromHandle(HWND(window as *mut _)).ok()?;
            let condition = if firefox { &self.firefox_bar } else { &self.text_box };
            root.FindFirst(TreeScope_Descendants, condition).ok()
        }
    }
}