//! The displays attached to the Mac and how large their points really are
//! (`docs/plans/editor-look.md`, LK0, K9): the UI grows on a screen whose
//! points are small — a 5K monitor run near its native resolution — so its
//! text reads as large as on a MacBook.
//!
//! * **CoreGraphics is declared here, not pulled in as a crate**, like CoreText
//!   in [`crate::fonts`]: three functions, linked from the system framework.
//! * **Bounds are in points, in the global display space** — the coordinates
//!   the webview's `window.screenX`/`screenY` use — so the page can tell which
//!   display its window is on.
//! * **The physical size comes from the display's EDID** (`CGDisplayScreenSize`);
//!   a display that does not report one (a projector, some adapters) gets no
//!   density, and the UI stays at its normal size there.

/// One display: where it is (points, global space) and how big it is (millimetres).
#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Physical width; 0 when the display does not say.
    pub width_mm: f64,
    /// A laptop's own screen, looked at from closer than a desk monitor.
    pub builtin: bool,
}

/// Points per inch macOS is designed for, where the UI scale is 1: a MacBook
/// in its standard resolution, and a desk monitor such as the Studio Display
/// — which is looked at from further away, so its points are larger.
pub const BUILTIN_POINTS_PER_INCH: f64 = 127.0;
pub const EXTERNAL_POINTS_PER_INCH: f64 = 110.0;
/// The UI never shrinks below its designed size, and grows at most this much.
pub const MIN_SCALE: f64 = 1.0;
pub const MAX_SCALE: f64 = 1.6;
/// Scales are rounded to this step, so a display's scale does not wobble.
const SCALE_STEP: f64 = 0.05;
const MM_PER_INCH: f64 = 25.4;

impl Display {
    /// How many points fit in an inch; `None` without a physical size.
    pub fn points_per_inch(&self) -> Option<f64> {
        (self.width_mm > 0.0 && self.width > 0.0)
            .then(|| self.width / (self.width_mm / MM_PER_INCH))
    }

    /// The UI scale for this display (K9): its density against the one macOS
    /// is designed for on this kind of display, within
    /// [`MIN_SCALE`]..=[`MAX_SCALE`], in steps of 0.05; 1 without a density.
    pub fn ui_scale(&self) -> f64 {
        let reference = if self.builtin {
            BUILTIN_POINTS_PER_INCH
        } else {
            EXTERNAL_POINTS_PER_INCH
        };
        self.points_per_inch().map_or(MIN_SCALE, |ppi| {
            let raw = (ppi / reference).clamp(MIN_SCALE, MAX_SCALE);
            (raw / SCALE_STEP).round() * SCALE_STEP
        })
    }
}

/// Every active display; empty off macOS or when CoreGraphics fails.
#[cfg(not(target_os = "macos"))]
pub fn displays() -> Vec<Display> {
    Vec::new()
}

/// Every active display; empty when CoreGraphics fails.
#[cfg(target_os = "macos")]
pub fn displays() -> Vec<Display> {
    /// More displays than a Mac can drive; the list is cut there.
    const MAX_DISPLAYS: u32 = 16;
    let mut ids = [0u32; MAX_DISPLAYS as usize];
    let mut count = 0u32;
    // SAFETY: `ids` holds `MAX_DISPLAYS` entries and CoreGraphics writes at most that many.
    let status = unsafe { ffi::CGGetActiveDisplayList(MAX_DISPLAYS, ids.as_mut_ptr(), &mut count) };
    if status != 0 {
        return Vec::new();
    }
    ids[..(count.min(MAX_DISPLAYS) as usize)]
        .iter()
        .map(|&id| {
            // SAFETY: plain value-returning queries on an id CoreGraphics just listed.
            let (bounds, size, builtin) = unsafe {
                (
                    ffi::CGDisplayBounds(id),
                    ffi::CGDisplayScreenSize(id),
                    ffi::CGDisplayIsBuiltin(id) != 0,
                )
            };
            Display {
                x: bounds.origin.x,
                y: bounds.origin.y,
                width: bounds.size.width,
                height: bounds.size.height,
                width_mm: size.width,
                builtin,
            }
        })
        .collect()
}

#[cfg(target_os = "macos")]
mod ffi {
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        pub fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
        pub fn CGDisplayBounds(display: u32) -> CGRect;
        pub fn CGDisplayScreenSize(display: u32) -> CGSize;
        pub fn CGDisplayIsBuiltin(display: u32) -> u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(width: f64, width_mm: f64) -> Display {
        Display {
            x: 0.0,
            y: 0.0,
            width,
            height: 1000.0,
            width_mm,
            builtin: false,
        }
    }

    #[test]
    fn a_dense_display_scales_up_within_the_limits() {
        // A MacBook Pro 14" in its standard resolution: 1512 points over ~302 mm.
        assert_eq!(
            Display {
                builtin: true,
                ..display(1512.0, 302.0)
            }
            .ui_scale(),
            1.0
        );
        // A Studio Display at its default 2560 points over ~597 mm: what macOS is designed for.
        assert_eq!(display(2560.0, 597.0).ui_scale(), 1.0);
        // A 40" 5K2K monitor at native 5120 points over ~934 mm → ~139 ppi.
        let wide = display(5120.0, 934.0);
        assert!((wide.points_per_inch().unwrap() - 139.2).abs() < 0.1);
        assert!((wide.ui_scale() - 1.25).abs() < 1e-9);
        // The same density on a laptop reads larger: it is closer to the eye.
        assert!(
            (Display {
                builtin: true,
                ..wide.clone()
            }
            .ui_scale()
                - 1.1)
                .abs()
                < 1e-9
        );
        // Coarser than designed: never smaller; absurdly dense: capped.
        assert_eq!(display(1920.0, 600.0).ui_scale(), MIN_SCALE);
        assert_eq!(display(8000.0, 300.0).ui_scale(), MAX_SCALE);
    }

    #[test]
    fn a_display_without_a_physical_size_stays_at_one() {
        assert_eq!(display(3840.0, 0.0).points_per_inch(), None);
        assert_eq!(display(3840.0, 0.0).ui_scale(), 1.0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_macs_own_displays_are_listed() {
        // A headless build machine may have none; any that are listed have a size.
        for d in displays() {
            println!(
                "{d:?}: {:?} ppi, scale {}",
                d.points_per_inch(),
                d.ui_scale()
            );
            assert!(d.width > 0.0 && d.height > 0.0, "{d:?}");
        }
    }
}
