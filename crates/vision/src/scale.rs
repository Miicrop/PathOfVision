//! One scale factor between the 1440 px reference layout and the game client.
//!
//! PoE lays its UI out relative to the client height, so every pixel constant is written
//! for a 1440 px tall client and converted to native pixels where it is used.

/// Client height all templates, offsets and thresholds were made at.
pub const REFERENCE_HEIGHT: f64 = 1440.0;

/// Native pixels per reference pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale(pub f64);

impl Scale {
    pub const REFERENCE: Scale = Scale(1.0);

    /// Scale of a client `w x h` px: its height over the reference height.
    pub fn for_client(_w: usize, h: usize) -> Scale {
        Scale(h as f64 / REFERENCE_HEIGHT)
    }

    /// Reference px to native px.
    pub fn px(self, reference_px: f64) -> f64 {
        reference_px * self.0
    }

    /// Reference px to native px, rounded.
    pub fn px_i(self, reference_px: i32) -> i32 {
        self.px(f64::from(reference_px)).round() as i32
    }

    /// Narrower than 16:9 (e.g. 16:10); PoE may limit the UI by width there.
    pub fn is_narrow(w: usize, h: usize) -> bool {
        w * 9 < h * 16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_client_uses_the_height() {
        assert_eq!(Scale::for_client(3440, 1440), Scale(1.0));
        assert_eq!(Scale::for_client(1920, 1080), Scale(0.75));
        assert_eq!(Scale::for_client(3840, 2160), Scale(1.5));
        assert_eq!(Scale::REFERENCE, Scale(1.0));
    }

    #[test]
    fn px_converts_reference_to_native() {
        assert_eq!(Scale(0.75).px_i(63), 47);
        assert_eq!(Scale(0.75).px(220.0), 165.0);
        assert_eq!(Scale::REFERENCE.px_i(650), 650);
    }

    #[test]
    fn narrow_is_below_16_by_9() {
        assert!(Scale::is_narrow(1920, 1200));
        assert!(!Scale::is_narrow(1920, 1080));
        assert!(!Scale::is_narrow(3440, 1440));
    }
}
