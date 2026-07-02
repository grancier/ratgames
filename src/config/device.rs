//! Responsive device classes: window-width breakpoints and their preset sizes.
//!
//! A game or example declares its target device in
//! [`WindowConfig`](super::WindowConfig), and the window opens at that class's
//! [`preset_size`](DeviceClass::preset_size) unless an explicit width/height
//! overrides it. At runtime [`for_width`](DeviceClass::for_width) reclassifies the
//! *live* window width, so the presentation can adapt when the window is resized
//! across a breakpoint.

use crate::geometry::Size;

/// A responsive breakpoint, classified by window width in device pixels.
///
/// The bands are contiguous (closing the gaps in the informal `<768` / `768–1024`
/// / `>1024` ranges): Mobile is `width < 768`, Tablet is `768..=1024`, and Desktop
/// is `width >= 1025`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceClass {
    /// Narrow window: `width < 768`.
    Mobile,
    /// Medium window: `768 <= width <= 1024`.
    Tablet,
    /// Wide window: `width >= 1025`. The default.
    #[default]
    Desktop,
}

impl DeviceClass {
    /// First width (device px) of the Tablet band; anything narrower is Mobile.
    pub const TABLET_MIN_PX: u32 = 768;
    /// First width (device px) of the Desktop band; the Tablet band runs up to one
    /// below this.
    pub const DESKTOP_MIN_PX: u32 = 1025;

    /// Classify a window width (device px) into its device class.
    #[must_use]
    pub fn for_width(width: u32) -> Self {
        if width >= Self::DESKTOP_MIN_PX {
            Self::Desktop
        } else if width >= Self::TABLET_MIN_PX {
            Self::Tablet
        } else {
            Self::Mobile
        }
    }

    /// A representative initial window size (device px) for the class, used when a
    /// [`WindowConfig`](super::WindowConfig) selects a class without an explicit
    /// width/height. Each preset's width falls inside its own band, so opening at
    /// the preset and reclassifying by width agree.
    #[must_use]
    pub fn preset_size(self) -> Size {
        match self {
            Self::Mobile => Size::new(360, 640),
            Self::Tablet => Size::new(768, 1024),
            Self::Desktop => Size::new(1280, 720),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_classification_covers_the_bands_and_boundaries() {
        assert_eq!(DeviceClass::for_width(0), DeviceClass::Mobile);
        assert_eq!(DeviceClass::for_width(767), DeviceClass::Mobile);
        assert_eq!(DeviceClass::for_width(768), DeviceClass::Tablet);
        assert_eq!(DeviceClass::for_width(1024), DeviceClass::Tablet);
        assert_eq!(DeviceClass::for_width(1025), DeviceClass::Desktop);
        assert_eq!(DeviceClass::for_width(1280), DeviceClass::Desktop);
    }

    #[test]
    fn each_preset_width_classifies_back_to_its_own_class() {
        for class in [
            DeviceClass::Mobile,
            DeviceClass::Tablet,
            DeviceClass::Desktop,
        ] {
            assert_eq!(DeviceClass::for_width(class.preset_size().w), class);
        }
    }

    #[test]
    fn default_is_desktop_at_1280x720() {
        assert_eq!(DeviceClass::default(), DeviceClass::Desktop);
        assert_eq!(DeviceClass::Desktop.preset_size(), Size::new(1280, 720));
    }

    #[test]
    fn serde_uses_snake_case_names() {
        assert_eq!(
            serde_json::to_string(&DeviceClass::Tablet).expect("serialize"),
            "\"tablet\""
        );
        let parsed: DeviceClass = serde_json::from_str("\"mobile\"").expect("deserialize");
        assert_eq!(parsed, DeviceClass::Mobile);
    }
}
