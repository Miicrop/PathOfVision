//! Whether a game client size is one the scanner was verified at.
use serde::Serialize;
use vision::Scale;

/// Client heights (px) with a real-screenshot acceptance run. Widened as sets are verified.
pub const VERIFIED_MIN_H: u32 = 1440;
pub const VERIFIED_MAX_H: u32 = 1440;
/// Above this height (or below the slack-reduced minimum) a size is unsupported.
pub const UNTESTED_MAX_H: u32 = 2160;
/// A windowed client is shorter than its monitor (title bar, taskbar); heights down to this
/// fraction of `VERIFIED_MIN_H` still count.
pub const WINDOWED_SLACK: f64 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Support {
    Verified,
    Untested,
    Unsupported,
}

/// Support level of a `w x h` client; `try_anyway` turns `Unsupported` into `Untested`.
pub fn support(w: u32, h: u32, try_anyway: bool) -> Support {
    let floor = (f64::from(VERIFIED_MIN_H) * WINDOWED_SLACK).ceil() as u32;
    let narrow = Scale::is_narrow(w as usize, h as usize);
    let level = if (floor..=UNTESTED_MAX_H).contains(&h) {
        if !narrow && h <= VERIFIED_MAX_H { Support::Verified } else { Support::Untested }
    } else {
        Support::Unsupported
    };
    if level == Support::Unsupported && try_anyway { Support::Untested } else { level }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_heights() {
        assert_eq!(support(3440, VERIFIED_MIN_H, false), Support::Verified);
        assert_eq!(support(3840, UNTESTED_MAX_H, false), Support::Untested);
        assert_eq!(support(1280, 720, false), Support::Unsupported);
        assert_eq!(support(1280, 720, true), Support::Untested);
        assert_eq!(support(3840, UNTESTED_MAX_H, true), Support::Untested);
        assert_eq!(support(3440, VERIFIED_MIN_H, true), Support::Verified);
        assert_eq!(support(7680, 4320, false), Support::Unsupported);
    }

    #[test]
    fn windowed_1080_monitor_is_verified() {
        let h = (VERIFIED_MIN_H as f64 * 0.94) as u32;
        assert_eq!(support(3440, h, false), Support::Verified);
    }

    #[test]
    fn narrow_is_untested() {
        assert_eq!(support(2560, 1600, false), Support::Untested);
        // 16:10 at a verified height is still untested.
        assert_eq!(support(VERIFIED_MIN_H * 16 / 10, VERIFIED_MIN_H, false), Support::Untested);
    }
}
