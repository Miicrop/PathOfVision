//! Whether a game client size is one the scanner was verified at.
use serde::Serialize;

/// Client heights (px) with a real-screenshot acceptance run (1920x1080, 1920x1200,
/// 2560x1353 and 3440x1440). Widened as sets are verified.
pub const VERIFIED_MIN_H: u32 = 1080;
pub const VERIFIED_MAX_H: u32 = 1440;
/// Narrowest verified aspect ratio, as width:height (16:10, from 1920x1200). Narrower clients
/// (4:3, 5:4) are untested.
pub const VERIFIED_MIN_ASPECT: (u32, u32) = (16, 10);
/// Widest verified aspect ratio (w / h). Wider clients (a window stretched over an ultrawide
/// with reduced height, which PoE pillarboxes) have no passing set and are untested.
pub const VERIFIED_MAX_ASPECT: f64 = 2.6;
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
    let narrow = u64::from(w) * u64::from(VERIFIED_MIN_ASPECT.1) < u64::from(h) * u64::from(VERIFIED_MIN_ASPECT.0);
    let too_wide = f64::from(w) > f64::from(h) * VERIFIED_MAX_ASPECT;
    let level = if f64::from(h) >= f64::from(VERIFIED_MIN_H) * WINDOWED_SLACK && h <= UNTESTED_MAX_H {
        if !narrow && !too_wide && h <= VERIFIED_MAX_H { Support::Verified } else { Support::Untested }
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
        assert_eq!(support(1920, VERIFIED_MIN_H, false), Support::Verified);
        assert_eq!(support(3840, UNTESTED_MAX_H, false), Support::Untested);
        assert_eq!(support(1280, 720, false), Support::Unsupported);
        assert_eq!(support(1280, 720, true), Support::Untested);
        assert_eq!(support(3840, UNTESTED_MAX_H, true), Support::Untested);
        assert_eq!(support(1920, VERIFIED_MIN_H, true), Support::Verified);
        assert_eq!(support(7680, 4320, false), Support::Unsupported);
    }

    #[test]
    fn windowed_1080_monitor_is_verified() {
        let h = (VERIFIED_MIN_H as f64 * 0.94) as u32;
        assert_eq!(support(1920, h, false), Support::Verified);
    }

    #[test]
    fn the_slack_floor_is_exact() {
        let floor = (f64::from(VERIFIED_MIN_H) * WINDOWED_SLACK).ceil() as u32;
        assert_eq!(support(1920, floor, false), Support::Verified);
        assert_eq!(support(1920, floor - 1, false), Support::Unsupported);
    }

    #[test]
    fn verified_heights() {
        assert_eq!(support(1920, 1080, false), Support::Verified);
        assert_eq!(support(2560, 1353, false), Support::Verified);
        assert_eq!(support(3440, 1440, false), Support::Verified);
        // A windowed client on a 1080 monitor (title bar and taskbar take ~10 %).
        assert_eq!(support(1920, 972, false), Support::Verified);
        assert_eq!(support(1920, 971, false), Support::Unsupported);
    }

    #[test]
    fn sixteen_by_ten_is_verified() {
        assert_eq!(support(1920, 1200, false), Support::Verified);
        assert_eq!(support(1728, 1080, false), Support::Verified);
        // Above the verified heights it is untested like any other aspect.
        assert_eq!(support(2560, 1600, false), Support::Untested);
    }

    #[test]
    fn wider_than_verified_is_untested() {
        // Window stretched over a 3440 monitor at reduced height (pillarboxed, aspect 3.03).
        assert_eq!(support(3435, 1132, false), Support::Untested);
        // A windowed 3440x1440 monitor (aspect 2.49) stays verified.
        assert_eq!(support(3440, 1380, false), Support::Verified);
    }

    #[test]
    fn narrower_than_16_by_10_is_untested() {
        assert_eq!(support(1440, 1080, false), Support::Untested);
        assert_eq!(support(1280, 1024, false), Support::Untested);
        assert_eq!(support(1727, 1080, false), Support::Untested);
    }
}
