//! The fonts installed on the Mac, through CoreText (`docs/plans/editor.md`,
//! ED5, T10, T16): every family with the weights it really has, and whether
//! it is monospaced — for the editor's and the terminal's font pickers.
//!
//! * **CoreText is declared here, not pulled in as a crate**: the handful of
//!   functions below, linked from the system frameworks, keep this crate free
//!   of dependencies like the rest of it. Every object CoreText hands back as
//!   "Create"/"Copy" is released exactly once ([`Owned`]).
//! * **Upright faces only**: italics are left out, the editor draws none.
//! * **Weights are CSS weights** (100–900), mapped from CoreText's own scale
//!   (-1…1) to the nearest of Apple's named weights, the way WebKit does —
//!   the webview then draws that very face for `font-weight`.
//! * **Private and unusable names are left out**: families starting with `.`
//!   (the system's own UI fonts, which a page cannot name) and names a CSS
//!   `font-family` string could be broken out of ([`usable_name`]).

use std::collections::BTreeMap;

/// One installed family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledFamily {
    pub family: String,
    /// CSS weights of its upright faces, ascending, each once.
    pub weights: Vec<u16>,
    /// Every upright face is monospaced.
    pub monospace: bool,
}

/// Longest family name accepted; real ones are far shorter.
pub const MAX_FAMILY_NAME: usize = 128;

/// CoreText's weight scale → CSS weight: Apple's named weights (`NSFontWeight*`).
const WEIGHT_ANCHORS: [(f64, u16); 9] = [
    (-0.8, 100),
    (-0.6, 200),
    (-0.4, 300),
    (0.0, 400),
    (0.23, 500),
    (0.3, 600),
    (0.4, 700),
    (0.56, 800),
    (0.62, 900),
];

/// The CSS weight nearest to CoreText weight `ct`.
pub fn css_weight(ct: f64) -> u16 {
    WEIGHT_ANCHORS
        .iter()
        .min_by(|a, b| (a.0 - ct).abs().total_cmp(&(b.0 - ct).abs()))
        .map_or(400, |&(_, css)| css)
}

/// Whether `name` can be a family the webview is told to draw with: not
/// empty, not too long, not a private system font, and nothing that could end
/// a quoted CSS string or a declaration (`"`, `'`, `\`, `;`, braces, control
/// characters).
pub fn usable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_FAMILY_NAME
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '\'' | '\\' | ';' | '{' | '}' | '<' | '>'))
}

/// One face as CoreText describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub family: String,
    /// CoreText's weight, -1…1.
    pub weight: f64,
    pub italic: bool,
    pub monospace: bool,
}

/// Faces grouped into families, sorted by name (case-insensitively).
pub fn families(faces: impl IntoIterator<Item = Face>) -> Vec<InstalledFamily> {
    let mut by_name: BTreeMap<String, InstalledFamily> = BTreeMap::new();
    for face in faces {
        if face.italic || !usable_name(&face.family) {
            continue;
        }
        let key = face.family.to_lowercase();
        let entry = by_name.entry(key).or_insert_with(|| InstalledFamily {
            family: face.family.clone(),
            weights: Vec::new(),
            monospace: true,
        });
        let weight = css_weight(face.weight);
        if !entry.weights.contains(&weight) {
            entry.weights.push(weight);
        }
        entry.monospace &= face.monospace;
    }
    by_name
        .into_values()
        .map(|mut f| {
            f.weights.sort_unstable();
            f
        })
        .collect()
}

/// Every installed family (empty off macOS).
pub fn installed_families() -> Vec<InstalledFamily> {
    families(installed_faces())
}

#[cfg(not(target_os = "macos"))]
fn installed_faces() -> Vec<Face> {
    Vec::new()
}

#[cfg(target_os = "macos")]
fn installed_faces() -> Vec<Face> {
    ct::installed_faces()
}

#[cfg(target_os = "macos")]
mod ct {
    //! The CoreText and CoreFoundation calls, and nothing else.

    use std::ffi::{c_char, c_void};

    use super::Face;

    type CFTypeRef = *const c_void;
    type CFIndex = isize;

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_CF_NUMBER_SINT64_TYPE: CFIndex = 4;
    const K_CF_NUMBER_FLOAT64_TYPE: CFIndex = 6;
    const K_CT_FONT_ITALIC_TRAIT: i64 = 1 << 0;
    const K_CT_FONT_MONOSPACE_TRAIT: i64 = 1 << 10;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: CFTypeRef);
        fn CFArrayGetCount(array: CFTypeRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(array: CFTypeRef, index: CFIndex) -> CFTypeRef;
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFDictionaryGetTypeID() -> usize;
        fn CFNumberGetTypeID() -> usize;
        fn CFStringGetLength(string: CFTypeRef) -> CFIndex;
        fn CFStringGetMaximumSizeForEncoding(length: CFIndex, encoding: u32) -> CFIndex;
        fn CFStringGetCString(
            string: CFTypeRef,
            buffer: *mut c_char,
            size: CFIndex,
            encoding: u32,
        ) -> u8;
        fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
        fn CFNumberGetValue(number: CFTypeRef, kind: CFIndex, out: *mut c_void) -> u8;
    }

    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        static kCTFontFamilyNameAttribute: CFTypeRef;
        static kCTFontTraitsAttribute: CFTypeRef;
        static kCTFontWeightTrait: CFTypeRef;
        static kCTFontSymbolicTrait: CFTypeRef;
        fn CTFontCollectionCreateFromAvailableFonts(options: CFTypeRef) -> CFTypeRef;
        fn CTFontCollectionCreateMatchingFontDescriptors(collection: CFTypeRef) -> CFTypeRef;
        fn CTFontDescriptorCopyAttribute(descriptor: CFTypeRef, attribute: CFTypeRef) -> CFTypeRef;
    }

    /// An object from a Create/Copy call, released when dropped.
    struct Owned(CFTypeRef);

    impl Owned {
        /// `None` for a null result.
        fn new(cf: CFTypeRef) -> Option<Self> {
            (!cf.is_null()).then_some(Self(cf))
        }
    }

    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: `self.0` came from a Create/Copy call (so we own one reference) and is not null.
            unsafe { CFRelease(self.0) }
        }
    }

    /// Whether `cf` (not null) is of the type `type_id` names.
    fn is(cf: CFTypeRef, type_id: usize) -> bool {
        // SAFETY: `cf` is a valid, non-null CoreFoundation object.
        !cf.is_null() && unsafe { CFGetTypeID(cf) } == type_id
    }

    /// A CFString's text, or `None` if it is not one.
    fn string(cf: CFTypeRef) -> Option<String> {
        // SAFETY: type ids are plain queries; `CFStringGetCString` writes at most `size` bytes
        // into `buffer`, which has that many, and NUL-terminates on success.
        unsafe {
            if !is(cf, CFStringGetTypeID()) {
                return None;
            }
            let size =
                CFStringGetMaximumSizeForEncoding(CFStringGetLength(cf), K_CF_STRING_ENCODING_UTF8)
                    + 1;
            let mut buffer = vec![0u8; usize::try_from(size).ok()?];
            if CFStringGetCString(
                cf,
                buffer.as_mut_ptr().cast(),
                size,
                K_CF_STRING_ENCODING_UTF8,
            ) == 0
            {
                return None;
            }
            let end = buffer.iter().position(|&b| b == 0)?;
            buffer.truncate(end);
            String::from_utf8(buffer).ok()
        }
    }

    /// A CFNumber in a traits dictionary, as `T` of CoreFoundation kind `kind`.
    fn number<T: Default>(dict: CFTypeRef, key: CFTypeRef, kind: CFIndex) -> Option<T> {
        // SAFETY: `dict` is a CFDictionary (checked by the caller); the value is borrowed (Get rule)
        // and checked to be a CFNumber before it is read into a `T` of the size `kind` names.
        unsafe {
            let value = CFDictionaryGetValue(dict, key);
            if !is(value, CFNumberGetTypeID()) {
                return None;
            }
            let mut out = T::default();
            (CFNumberGetValue(value, kind, (&raw mut out).cast()) != 0).then_some(out)
        }
    }

    /// Every face of every installed font.
    pub(super) fn installed_faces() -> Vec<Face> {
        // SAFETY: the collection and the descriptor array are owned (`Owned` releases them); the
        // descriptors inside the array are borrowed from it and used only while it lives; every
        // copied attribute is owned; the `kCT…` keys are constants CoreText exports.
        unsafe {
            let Some(collection) =
                Owned::new(CTFontCollectionCreateFromAvailableFonts(std::ptr::null()))
            else {
                return Vec::new();
            };
            let Some(descriptors) =
                Owned::new(CTFontCollectionCreateMatchingFontDescriptors(collection.0))
            else {
                return Vec::new();
            };
            let count = CFArrayGetCount(descriptors.0);
            let mut faces = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
            for i in 0..count {
                let descriptor = CFArrayGetValueAtIndex(descriptors.0, i);
                if descriptor.is_null() {
                    continue;
                }
                let Some(name) = Owned::new(CTFontDescriptorCopyAttribute(
                    descriptor,
                    kCTFontFamilyNameAttribute,
                )) else {
                    continue;
                };
                let Some(family) = string(name.0) else {
                    continue;
                };
                let traits = Owned::new(CTFontDescriptorCopyAttribute(
                    descriptor,
                    kCTFontTraitsAttribute,
                ));
                let traits = traits.filter(|t| is(t.0, CFDictionaryGetTypeID()));
                let (weight, symbolic) = traits.as_ref().map_or((0.0, 0), |t| {
                    (
                        number::<f64>(t.0, kCTFontWeightTrait, K_CF_NUMBER_FLOAT64_TYPE)
                            .unwrap_or(0.0),
                        number::<i64>(t.0, kCTFontSymbolicTrait, K_CF_NUMBER_SINT64_TYPE)
                            .unwrap_or(0),
                    )
                });
                faces.push(Face {
                    family,
                    weight,
                    italic: symbolic & K_CT_FONT_ITALIC_TRAIT != 0,
                    monospace: symbolic & K_CT_FONT_MONOSPACE_TRAIT != 0,
                });
            }
            faces
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(family: &str, weight: f64, italic: bool, monospace: bool) -> Face {
        Face {
            family: family.into(),
            weight,
            italic,
            monospace,
        }
    }

    #[test]
    fn maps_coretext_weights_to_the_nearest_css_weight() {
        assert_eq!(css_weight(0.0), 400);
        assert_eq!(css_weight(-0.8), 100);
        assert_eq!(css_weight(0.4), 700);
        assert_eq!(css_weight(0.37), 700);
        assert_eq!(css_weight(1.0), 900);
    }

    #[test]
    fn groups_faces_into_families_leaving_out_italics_and_unusable_names() {
        let list = families([
            face("Menlo", 0.4, false, true),
            face("Menlo", 0.0, false, true),
            face("Menlo", 0.0, true, true),
            face("Helvetica", 0.0, false, false),
            face(".SF NS Mono", 0.0, false, true),
            face("Evil\"; x", 0.0, false, true),
        ]);
        assert_eq!(
            list,
            vec![
                InstalledFamily {
                    family: "Helvetica".into(),
                    weights: vec![400],
                    monospace: false
                },
                InstalledFamily {
                    family: "Menlo".into(),
                    weights: vec![400, 700],
                    monospace: true
                },
            ]
        );
    }

    #[test]
    fn a_family_is_monospace_only_if_every_face_is() {
        let list = families([
            face("Mixed", 0.0, false, true),
            face("Mixed", 0.4, false, false),
        ]);
        assert!(!list[0].monospace);
    }

    #[test]
    fn rejects_names_that_could_break_out_of_css() {
        assert!(usable_name("JetBrains Mono"));
        assert!(usable_name("Hiragino Kaku Gothic ProN"));
        for bad in [
            "",
            ".SFNS",
            "a\"b",
            "a'b",
            "a\\b",
            "a;b",
            "a{b",
            "a\nb",
            &"x".repeat(129),
        ] {
            assert!(!usable_name(bad), "{bad:?}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn finds_the_macs_own_fonts() {
        let list = installed_families();
        let menlo = list
            .iter()
            .find(|f| f.family == "Menlo")
            .expect("Menlo ships with macOS");
        assert!(menlo.monospace);
        assert!(menlo.weights.contains(&400) && menlo.weights.contains(&700));
        let helvetica = list
            .iter()
            .find(|f| f.family == "Helvetica")
            .expect("Helvetica ships with macOS");
        assert!(!helvetica.monospace);
    }
}
