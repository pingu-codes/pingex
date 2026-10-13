//! Which files count as images, shared by attachment staging and the thread
//! image viewer so the two can never disagree about an extension.

use std::path::Path;

/// Image extensions (lowercase) and the MIME type each one is staged as.
/// The frontend mirror is `IMAGE_EXTENSIONS` in `src/lib/utils/media.ts`.
pub(crate) const IMAGE_EXTENSIONS: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
    ("tif", "image/tiff"),
    ("tiff", "image/tiff"),
    ("heic", "image/heic"),
    ("avif", "image/avif"),
    ("ico", "image/x-icon"),
    ("svg", "image/svg+xml"),
];

/// The MIME type for an image extension, matched case-insensitively.
pub(crate) fn image_mime(extension: &str) -> Option<&'static str> {
    IMAGE_EXTENSIONS
        .iter()
        .find(|(ext, _)| ext.eq_ignore_ascii_case(extension))
        .map(|(_, mime)| *mime)
}

/// Whether `path` ends in an image extension. Works on host paths as strings,
/// so a Linux path is judged the same way on a Windows build.
pub(crate) fn is_image_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| image_mime(ext).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_extensions_case_insensitively() {
        assert_eq!(image_mime("png"), Some("image/png"));
        assert_eq!(image_mime("JPG"), Some("image/jpeg"));
        assert_eq!(image_mime("heic"), Some("image/heic"));
        assert_eq!(image_mime("avif"), Some("image/avif"));
        assert_eq!(image_mime("ico"), Some("image/x-icon"));
        assert_eq!(image_mime("txt"), None);
        assert_eq!(image_mime(""), None);
    }

    #[test]
    fn extensions_are_lowercase_and_unique() {
        for (i, (ext, mime)) in IMAGE_EXTENSIONS.iter().enumerate() {
            assert_eq!(*ext, ext.to_ascii_lowercase());
            assert!(mime.starts_with("image/"));
            assert!(IMAGE_EXTENSIONS[i + 1..]
                .iter()
                .all(|(other, _)| other != ext));
        }
    }

    #[test]
    fn judges_paths_by_extension() {
        assert!(is_image_path("/tmp/shot.PNG"));
        assert!(is_image_path("~/a b/diff.webp"));
        assert!(is_image_path("/tmp/photo.heic"));
        assert!(!is_image_path("/home/me/.ssh/id_ed25519"));
        assert!(!is_image_path("/tmp/png"));
    }
}
