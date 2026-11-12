//! Phones get the read-only viewer (`viewer.html`, the `draftcraft-viewer` binary) instead of the
//! full app: a phone screen has no room for the ribbon and the panels, and the viewer downloads a
//! fraction of the full app. The site's host sends phones to the viewer before anything loads
//! (`vercel.json` rewrites by user agent); this check is the fallback for any other host, run as
//! the full app starts.
//!
//! `?desktop` keeps the full app on a phone; `?mobile` (or `?viewer`) opens the viewer anywhere.

/// What the browser says about the device.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Device {
    pub user_agent: String,
    /// `navigator.maxTouchPoints`.
    pub max_touch_points: i32,
    /// `(pointer: coarse)` matches: the main pointer is a finger.
    pub coarse_pointer: bool,
    /// The screen's shorter side in CSS pixels.
    pub short_side: f64,
}

/// A screen whose shorter side is below this many CSS pixels is a phone's (tablets start at 600).
pub const PHONE_SHORT_SIDE: f64 = 600.0;

/// Whether the URL query (`?a&b=c`) has the flag `name`, bare or with a value.
pub fn has_flag(query: &str, name: &str) -> bool {
    query.trim_start_matches('?').split('&').any(|p| p == name || p.strip_prefix(name).is_some_and(|rest| rest.starts_with('=')))
}

/// Whether the user agent names a phone (not a tablet: an Android tablet leaves out `Mobile`, an
/// iPad asks for the desktop site).
pub fn phone_user_agent(ua: &str) -> bool {
    let ua = ua.to_ascii_lowercase();
    ["iphone", "ipod", "windows phone", "iemobile", "opera mini", "blackberry"].iter().any(|k| ua.contains(k))
        || (ua.contains("android") && ua.contains("mobile"))
}

/// Whether this device should get the viewer: a phone by its user agent, or a small touch screen
/// whose main pointer is a finger. The URL's `?desktop` and `?mobile` flags decide first.
pub fn wants_viewer(query: &str, device: &Device) -> bool {
    if has_flag(query, "desktop") {
        return false;
    }
    if has_flag(query, "mobile") || has_flag(query, "viewer") {
        return true;
    }
    let small_touch = device.coarse_pointer
        && device.max_touch_points > 0
        && device.short_side.is_finite()
        && device.short_side > 0.0
        && device.short_side < PHONE_SHORT_SIDE;
    phone_user_agent(&device.user_agent) || small_touch
}

/// The viewer's address next to the full app's, keeping the query.
pub fn viewer_url(query: &str) -> String {
    format!("viewer.html{query}")
}

/// Send a phone to the viewer: `true` when the page is being replaced (the full app must not start).
#[cfg(target_arch = "wasm32")]
pub fn redirect_to_viewer() -> bool {
    let Some(window) = web_sys::window() else { return false };
    let query = window.location().search().unwrap_or_default();
    let navigator = window.navigator();
    let screen = window.screen().ok();
    let side = |f: fn(&web_sys::Screen) -> Result<i32, wasm_bindgen::JsValue>| screen.as_ref().and_then(|s| f(s).ok()).map(f64::from);
    let short_side = match (side(web_sys::Screen::width), side(web_sys::Screen::height)) {
        (Some(w), Some(h)) => w.min(h),
        _ => 0.0,
    };
    let device = Device {
        user_agent: navigator.user_agent().unwrap_or_default(),
        max_touch_points: navigator.max_touch_points(),
        coarse_pointer: window.match_media("(pointer: coarse)").ok().flatten().is_some_and(|m| m.matches()),
        short_side,
    };
    if !wants_viewer(&query, &device) {
        return false;
    }
    match window.location().replace(&viewer_url(&query)) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("could not open the phone viewer: {e:?}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(ua: &str, touch: i32, coarse: bool, short: f64) -> Device {
        Device { user_agent: ua.into(), max_touch_points: touch, coarse_pointer: coarse, short_side: short }
    }

    const IPHONE: &str =
        "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";
    const ANDROID_PHONE: &str = "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Mobile Safari/537.36";
    const ANDROID_TABLET: &str = "Mozilla/5.0 (Linux; Android 15; SM-X910) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
    const IPAD: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
    const MAC: &str = IPAD;

    /// Phones get the viewer by user agent; tablets and computers keep the full app.
    #[test]
    fn phones_get_the_viewer_tablets_and_computers_do_not() {
        assert!(wants_viewer("", &device(IPHONE, 5, true, 390.0)));
        assert!(wants_viewer("", &device(ANDROID_PHONE, 5, true, 412.0)));
        assert!(!wants_viewer("", &device(ANDROID_TABLET, 10, true, 800.0)));
        assert!(!wants_viewer("", &device(IPAD, 5, true, 820.0)));
        assert!(!wants_viewer("", &device(MAC, 0, false, 900.0)));
    }

    /// A small touch screen with a finger pointer is a phone even when its user agent says
    /// nothing (a phone asking for the desktop site); a small mouse window is not.
    #[test]
    fn a_small_touch_screen_is_a_phone() {
        assert!(wants_viewer("", &device(MAC, 5, true, 390.0)));
        assert!(!wants_viewer("", &device(MAC, 0, false, 390.0)));
        assert!(!wants_viewer("", &device(MAC, 5, true, f64::NAN)));
    }

    /// The URL flags decide before the device does.
    #[test]
    fn url_flags_override_the_device() {
        assert!(!wants_viewer("?desktop", &device(IPHONE, 5, true, 390.0)));
        assert!(!wants_viewer("?sample&desktop=1", &device(IPHONE, 5, true, 390.0)));
        assert!(wants_viewer("?mobile", &device(MAC, 0, false, 900.0)));
        assert!(wants_viewer("?viewer", &device(MAC, 0, false, 900.0)));
        assert!(!wants_viewer("?desktopx", &device(MAC, 0, false, 900.0)));
        assert!(!has_flag("?mobiles", "mobile"));
    }

    #[test]
    fn viewer_url_keeps_the_query() {
        assert_eq!(viewer_url("?sample"), "viewer.html?sample");
        assert_eq!(viewer_url(""), "viewer.html");
    }
}
