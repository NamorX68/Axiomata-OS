//! The owner's background image behind the glass (`docs/plans/glass.md`, G15): picked once through the native
//! dialog, checked to be a picture, copied into `~/.axiomata/wallpaper/` and from then on read only from there.
//!
//! The webview never names a path. It asks for a pick, gets the copied bytes back to show as a `blob:` URL, or clears
//! the image — so no command here lets it read a file of its choosing.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

/// The largest image taken: a 5K photo is well below it, a mistaken pick of a video is not. It is held three times
/// on its way to the screen (Rust, the IPC copy, the `Blob`), so the cap is kept modest.
const MAX_BYTES: u64 = 25 * 1024 * 1024;

/// The stored image's name inside the folder, without its extension.
const STEM: &str = "current";

/// Every extension an image is stored under, one per kind [`image_kind`] knows. No GIF: an animation under the ground's
/// blur would cost a frame's work for every frame of it.
const KINDS: [&str; 3] = ["png", "jpg", "webp"];

/// The still picture formats a webview shows, by the magic bytes they start with.
fn image_kind(head: &[u8]) -> Option<&'static str> {
    match head {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("webp"),
        _ => None,
    }
}

/// Reads `file` if it is a picture of a size worth showing; the error says why not, in the owner's words. Used for the
/// pick and again for every read of the copy, so what reaches the webview is an image even if the folder was touched.
fn read_image(file: fs::File) -> Result<(Vec<u8>, &'static str), String> {
    let mut bytes = Vec::new();
    // Bounded by the read itself, not by the size the metadata claimed before it: the file may grow meanwhile.
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("could not read the image: {err}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!(
            "the image is larger than {} MB",
            MAX_BYTES / (1024 * 1024)
        ));
    }
    let kind = image_kind(&bytes).ok_or("this is no PNG, JPEG or WebP image")?;
    Ok((bytes, kind))
}

/// Opens `path` without following a link at its end: a stored image that became a link is refused instead of read.
fn open_plain(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)
}

/// The stored image, if there is one: a plain file, never a link.
fn stored(dir: &Path) -> Option<PathBuf> {
    KINDS
        .into_iter()
        .map(|ext| dir.join(format!("{STEM}.{ext}")))
        .find(|path| fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file()))
}

/// Puts `bytes` in place of whatever image `dir` held: written beside it first and moved over the old one, so a failed
/// write leaves the old image; the other kinds go only once the new one is in place.
fn store(dir: &Path, bytes: &[u8], kind: &str) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    let target = dir.join(format!("{STEM}.{kind}"));
    let partial = dir.join(format!(".{STEM}.{kind}.partial"));
    remove_if_there(&partial)?;
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .and_then(|mut file| std::io::Write::write_all(&mut file, bytes))
        .and_then(|()| fs::rename(&partial, &target));
    if let Err(err) = written {
        let _ = fs::remove_file(&partial);
        return Err(err.to_string());
    }
    KINDS
        .into_iter()
        .filter(|ext| *ext != kind)
        .try_for_each(|ext| remove_if_there(&dir.join(format!("{STEM}.{ext}"))))
}

fn remove_if_there(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err.to_string()),
        _ => Ok(()),
    }
}

/// Removes the stored image; nothing stored is no error.
fn clear(dir: &Path) -> Result<(), String> {
    KINDS
        .into_iter()
        .try_for_each(|ext| remove_if_there(&dir.join(format!("{STEM}.{ext}"))))
}

/// The three commands one at a time: a pick and a clear must not interleave in the folder.
static FOLDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn in_folder<T>(work: impl FnOnce(&Path) -> Result<T, String>) -> Result<T, String> {
    let _turn = FOLDER.lock().unwrap_or_else(|poison| poison.into_inner());
    work(&axiomata_core::paths::wallpaper_dir())
}

/// Lets the owner pick a background image in the native dialog and keeps a copy of it. `false` when the dialog was
/// cancelled.
#[tauri::command]
pub async fn wallpaper_pick(app: AppHandle) -> Result<bool, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Choose a background image")
        .add_filter("Image", &["png", "jpg", "jpeg", "webp"])
        .pick_file(move |picked| {
            let _ = tx.send(picked.and_then(|p| p.into_path().ok()));
        });
    let Some(source) = rx.await.ok().flatten() else {
        return Ok(false);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let file =
            fs::File::open(&source).map_err(|err| format!("could not open the image: {err}"))?;
        let (bytes, kind) = read_image(file)?;
        in_folder(|dir| store(dir, &bytes, kind))
    })
    .await
    .map_err(|err| err.to_string())??;
    Ok(true)
}

/// The stored background image's bytes (raw, not JSON), or an empty body when there is none.
#[tauri::command]
pub async fn wallpaper_read() -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(|| {
        in_folder(|dir| match stored(dir) {
            Some(path) => {
                let file = open_plain(&path).map_err(|err| err.to_string())?;
                read_image(file).map(|(bytes, _)| Response::new(bytes))
            }
            None => Ok(Response::new(Vec::new())),
        })
    })
    .await
    .map_err(|err| err.to_string())?
}

/// Forgets the background image.
#[tauri::command]
pub async fn wallpaper_clear() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| in_folder(clear))
        .await
        .map_err(|err| err.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder of its own per test, removed afterwards.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("axiomata-wallpaper-{label}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];
    const JPG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0];

    #[test]
    fn knows_the_pictures_a_webview_shows_by_their_first_bytes() {
        assert_eq!(image_kind(PNG), Some("png"));
        assert_eq!(image_kind(JPG), Some("jpg"));
        assert_eq!(image_kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_kind(b"GIF89a"), None);
        assert_eq!(image_kind(b"<svg xmlns="), None);
        assert_eq!(image_kind(b""), None);
    }

    #[test]
    fn refuses_a_file_that_is_no_picture() {
        let dir = Scratch::new("no-picture");
        let source = dir.path().join("notes.png");
        fs::write(&source, b"just text").unwrap();
        assert!(
            read_image(fs::File::open(&source).unwrap())
                .unwrap_err()
                .contains("no PNG")
        );
    }

    #[test]
    fn takes_a_picture_and_names_its_kind() {
        let dir = Scratch::new("takes");
        let source = dir.path().join("photo.jpeg");
        fs::write(&source, JPG).unwrap();
        assert_eq!(
            read_image(fs::File::open(&source).unwrap()).unwrap(),
            (JPG.to_vec(), "jpg")
        );
    }

    #[test]
    fn a_new_image_replaces_the_old_one_even_of_another_kind() {
        let dir = Scratch::new("replaces");
        store(dir.path(), PNG, "png").unwrap();
        store(dir.path(), JPG, "jpg").unwrap();
        assert_eq!(stored(dir.path()), Some(dir.path().join("current.jpg")));
        assert!(!dir.path().join("current.png").exists());
        assert_eq!(fs::read(dir.path().join("current.jpg")).unwrap(), JPG);
    }

    #[test]
    fn a_stored_image_that_became_a_link_is_not_followed() {
        let dir = Scratch::new("link");
        let secret = dir.path().join("secret.png");
        fs::write(&secret, PNG).unwrap();
        std::os::unix::fs::symlink(&secret, dir.path().join("current.png")).unwrap();
        assert_eq!(stored(dir.path()), None);
        assert!(open_plain(&dir.path().join("current.png")).is_err());
    }

    #[test]
    fn clearing_removes_the_image_and_clearing_nothing_is_fine() {
        let dir = Scratch::new("clears");
        clear(dir.path()).unwrap();
        store(dir.path(), PNG, "png").unwrap();
        clear(dir.path()).unwrap();
        assert_eq!(stored(dir.path()), None);
    }
}
