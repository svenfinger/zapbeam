use serde::Serialize;
use std::{fs::File, io::Read, path::Path};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageFile {
    path: String,
    name: String,
    format: &'static str,
    size: u64,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Inspection {
    Image { file: ImageFile },
    Rejected { name: String, message: &'static str },
}

fn detect_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("PNG")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("JPEG")
    } else {
        None
    }
}

fn inspect(path: &Path) -> Result<ImageFile, &'static str> {
    if !path.is_absolute() {
        return Err("Choose a file using Add Files or drag it into the window.");
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| "File is unavailable. It may have moved or access was denied.")?;
    let metadata = canonical
        .metadata()
        .map_err(|_| "Could not read file information.")?;
    if !metadata.is_file() {
        return Err("Not a regular file. Add individual JPEG or PNG images.");
    }
    let mut source =
        File::open(&canonical).map_err(|_| "Could not open file. Check its read permissions.")?;
    let metadata = source
        .metadata()
        .map_err(|_| "Could not read file information.")?;
    let mut signature = [0; 8];
    // Read only a signature, never decode or load the full image.
    let count = source
        .by_ref()
        .take(8)
        .read(&mut signature)
        .map_err(|_| "Could not read the image header.")?;
    let format = detect_format(&signature[..count])
        .ok_or("Not a supported image. Only JPEG and PNG are supported.")?;
    let canonical = canonical
        .to_str()
        .ok_or("This file path cannot be represented. Try renaming the file.")?;
    Ok(ImageFile {
        path: canonical.to_owned(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        format,
        size: metadata.len(),
    })
}

#[tauri::command]
pub async fn inspect_files(paths: Vec<String>) -> Result<Vec<Inspection>, &'static str> {
    if paths.len() > 256 {
        return Err("Add at most 256 files per inspection.");
    }
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .iter()
            .map(|value| {
                let path = Path::new(value);
                match inspect(path) {
                    Ok(file) => Inspection::Image { file },
                    Err(message) => Inspection::Rejected {
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        message,
                    },
                }
            })
            .collect()
    })
    .await
    .map_err(|_| "Could not inspect these files. Please try again.")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signatures_not_extensions() {
        assert_eq!(detect_format(b"\x89PNG\r\n\x1a\n"), Some("PNG"));
        assert_eq!(detect_format(&[0xff, 0xd8, 0xff, 0xe1]), Some("JPEG"));
        for bytes in [
            b"GIF89a".as_slice(),
            b"hello.png",
            b"",
            b"\x89PNG",
            &[0xff, 0xd8],
        ] {
            assert_eq!(detect_format(bytes), None);
        }
    }
    #[test]
    fn inspection_handles_missing_directories_and_real_metadata() {
        let folder = std::env::temp_dir().join(format!("zapbeam-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        assert!(inspect(&folder).is_err());
        assert!(inspect(&folder.join("missing.png")).is_err());
        assert!(inspect(Path::new("relative.png")).is_err());
        let file = folder.join("image.txt");
        let bytes = b"\x89PNG\r\n\x1a\nfixture";
        std::fs::write(&file, bytes).unwrap();
        let result = inspect(&file).unwrap();
        assert_eq!(result.format, "PNG");
        assert_eq!(result.size, bytes.len() as u64);
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
        std::fs::write(&file, b"not an image").unwrap();
        assert!(inspect(&file).is_err());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
