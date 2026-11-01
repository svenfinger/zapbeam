//! Embedded metadata policy. Filesystem attributes have a separate preservation policy.
use super::*;

// OxiPNG Safe is the starting point, extended to retain gamma/chromaticities,
// background, significant bits, EXIF orientation and HDR display information.
const PNG_KEEP: [[u8; 4]; 12] = [
    *b"cICP", *b"iCCP", *b"sRGB", *b"pHYs", *b"gAMA", *b"cHRM", *b"bKGD", *b"sBIT", *b"eXIf",
    *b"mDCv", *b"cLLi", *b"tRNS",
];

pub(super) fn png_options() -> oxipng::StripChunks {
    oxipng::StripChunks::Keep(PNG_KEEP.into_iter().collect())
}

pub(super) fn png_expected(bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    png_metadata(bytes)?
        .into_iter()
        .filter_map(|chunk| {
            let name: [u8; 4] = chunk[4..8].try_into().unwrap();
            if name == *b"iTXt" && chunk[8..].starts_with(b"XML:com.adobe.xmp\0") {
                return match png_xmp(&chunk[8..chunk.len() - 4]) {
                    Ok(()) => None,
                    Err(e) => Some(Err(e)),
                };
            }
            if !PNG_KEEP.contains(&name) {
                return None;
            }
            Some(if name == *b"eXIf" {
                exif(&chunk[8..chunk.len() - 4]).map(|data| png_chunk(name, &data))
            } else {
                Ok(chunk)
            })
        })
        .collect()
}

fn png_chunk(name: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut result = (data.len() as u32).to_be_bytes().to_vec();
    result.extend(name);
    result.extend(data);
    // PNG's IEEE CRC-32. Small metadata only, outside the compression hot path.
    let mut crc = !0u32;
    for byte in &result[4..] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    result.extend((!crc).to_be_bytes());
    result
}

pub(super) fn png_input(source: &[u8]) -> Result<Vec<u8>> {
    let expected = png_expected(source)?;
    if expected.iter().filter(|c| &c[4..8] == b"eXIf").count() > 1 {
        return Err("Duplicate PNG EXIF chunk".into());
    }
    let mut output = source[..8].to_vec();
    let mut pos = 8;
    while pos < source.len() {
        let len = u32::from_be_bytes(source[pos..pos + 4].try_into()?) as usize;
        let end = pos + len + 12;
        let name = &source[pos + 4..pos + 8];
        if name == b"eXIf" {
            for chunk in expected.iter().filter(|c| &c[4..8] == b"eXIf") {
                output.extend(chunk);
            }
        } else {
            output.extend(&source[pos..end]);
        }
        pos = end;
    }
    Ok(output)
}

fn unsupported_xmp() -> crate::diagnostics::Error {
    failure(
        Category::Unsupported,
        "XMP contains unrecognized rendering or HDR instructions; original retained",
    )
}

pub(super) fn jpeg_expected(bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    jpeg_metadata(bytes)?.into_iter().filter_map(|marker| {
        let payload = &marker[3..];
        let data = match marker[0] {
            0xe0 if payload.starts_with(b"JFIF\0") => {
                // Retain density/aspect ratio; remove the embedded thumbnail.
                if payload.len() < 14 { return Some(Err("Truncated JFIF".into())); }
                let mut data = payload[..14].to_vec();
                data[12..14].fill(0);
                data
            }
            0xe1 if payload.starts_with(b"Exif\0\0") => match exif(&payload[6..]) {
                Ok(tiff) => [b"Exif\0\0".as_slice(), &tiff].concat(),
                Err(e) => return Some(Err(e)),
            },
            0xe1 if payload.starts_with(b"http://ns.adobe.com/xap/1.0/\0") => {
                return match disposable_xmp(&payload[29..]) { Ok(()) => None, Err(e) => Some(Err(e)) };
            }
            0xe1 if payload.starts_with(b"http://ns.adobe.com/") => return Some(Err(unsupported_xmp())),
            0xe2 if payload.starts_with(b"ICC_PROFILE\0") => return Some(Ok(marker)),
            0xee if payload.starts_with(b"Adobe") => return Some(Ok(marker)),
            // Comments, JFXX previews, IPTC/Photoshop and unrecognized APP payloads
            // are not assumed safe to discard if their semantics are unknown.
            0xfe => return None,
            0xe0 if payload.starts_with(b"JFXX\0") => return None,
            _ => return Some(Err(failure(Category::Unsupported, "Unknown JPEG application metadata; original retained pending a rendering policy"))),
        };
        let mut output = vec![marker[0]];
        output.extend(((data.len()+2) as u16).to_be_bytes());
        output.extend(data);
        Some(Ok(output))
    }).collect()
}

pub(super) fn jpeg_input(source: &[u8]) -> Result<Vec<u8>> {
    // Parse the complete original before removing anything, including signatures,
    // MPF and trailing structures after the first scan.
    let expected = jpeg_expected(source)?;
    let mut markers = expected.iter();
    let original = jpeg_metadata(source)?;
    let mut original_markers = original.iter();
    let mut output = source[..2].to_vec();
    let mut pos = 2;
    while pos < source.len() {
        let start = pos;
        if source[pos] != 255 {
            output.push(source[pos]);
            pos += 1;
            continue;
        }
        pos += 1;
        while source[pos] == 255 {
            pos += 1;
        }
        let marker = source[pos];
        pos += 1;
        if marker == 0 || marker == 0xd9 || (0xd0..=0xd7).contains(&marker) {
            output.extend(&source[start..pos]);
            continue;
        }
        let len = u16::from_be_bytes(source[pos..pos + 2].try_into()?) as usize;
        let end = pos + len;
        if (0xe0..=0xef).contains(&marker) || marker == 0xfe {
            let original = original_markers.next().ok_or("Missing JPEG marker")?;
            let payload = &original[3..];
            if marker != 0xfe
                && !(marker == 0xe0 && payload.starts_with(b"JFXX\0"))
                && !(marker == 0xe1 && payload.starts_with(b"http://ns.adobe.com/xap/1.0/\0"))
            {
                output.push(255);
                output.extend(markers.next().ok_or("Missing retained marker")?);
            }
        } else {
            output.extend(&source[start..end]);
        }
        pos = end;
    }
    Ok(output)
}

// Rebuild a small TIFF containing rendering tags only. Never copy an IFD, pointer,
// thumbnail, MakerNote, GPS block or unused padding wholesale. Retain byte order.
fn exif(source: &[u8]) -> Result<Vec<u8>> {
    let little = match source.get(..4) {
        Some(b"II\x2a\0") => true,
        Some(b"MM\0\x2a") => false,
        _ => return Err("Invalid EXIF TIFF header".into()),
    };
    let mut output = source[..4].to_vec();
    put32(&mut output, 8, little);
    let root = number(source, 4, 4, little)? as usize;
    if root < 8 {
        return Err("Invalid EXIF IFD offset".into());
    }
    copy_ifd(source, root, 0, little, &mut output)?;
    Ok(output)
}
fn number(bytes: &[u8], offset: usize, size: usize, little: bool) -> Result<u32> {
    let data = bytes
        .get(offset..offset.checked_add(size).ok_or("EXIF offset overflow")?)
        .ok_or("Truncated EXIF")?;
    Ok(if little {
        data.iter().rev().fold(0, |a, b| a * 256 + u32::from(*b))
    } else {
        data.iter().fold(0, |a, b| a * 256 + u32::from(*b))
    })
}
fn put32(output: &mut Vec<u8>, value: u32, little: bool) {
    output.extend(if little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    });
}
fn copy_ifd(
    source: &[u8],
    offset: usize,
    level: u8,
    little: bool,
    output: &mut Vec<u8>,
) -> Result<()> {
    let count = number(source, offset, 2, little)? as usize;
    if count > 4096 {
        return Err("Too many EXIF entries".into());
    }
    let end = offset
        .checked_add(2 + count * 12 + 4)
        .ok_or("EXIF overflow")?;
    if end > source.len() {
        return Err("Truncated EXIF directory".into());
    }
    let mut entries = Vec::new();
    let mut tags = std::collections::HashSet::new();
    for index in 0..count {
        let pos = offset + 2 + index * 12;
        let tag = number(source, pos, 2, little)?;
        let keep = match level {
            0 => matches!(
                tag,
                0x0112
                    | 0x011a
                    | 0x011b
                    | 0x0128
                    | 0x012d
                    | 0x013e
                    | 0x013f
                    | 0x0211
                    | 0x0212
                    | 0x0213
                    | 0x0214
                    | 0x8769
            ),
            1 => matches!(tag, 0xa001 | 0xa002 | 0xa003 | 0xa005 | 0xa500),
            2 => matches!(tag, 1 | 2),
            _ => false,
        };
        if !keep {
            continue;
        }
        if !tags.insert(tag) {
            return Err("Duplicate rendering EXIF tag".into());
        }
        let kind = number(source, pos + 2, 2, little)?;
        let count = number(source, pos + 4, 4, little)?;
        let unit = match kind {
            1 | 2 | 7 => 1,
            3 => 2,
            4 => 4,
            5 => 8,
            _ => return Err("Unsupported rendering EXIF type".into()),
        };
        let size = count.checked_mul(unit).ok_or("EXIF size overflow")? as usize;
        if size > 1024 || size == 0 {
            return Err("Invalid rendering EXIF size".into());
        }
        if tag == 0x0112
            && (kind != 3 || count != 1 || !(1..=8).contains(&number(source, pos + 8, 2, little)?))
        {
            return Err("Invalid EXIF orientation".into());
        }
        let pointer = matches!(tag, 0x8769 | 0xa005);
        if pointer && (kind != 4 || count != 1) {
            return Err("Invalid EXIF pointer".into());
        }
        let data_offset = if size > 4 {
            number(source, pos + 8, 4, little)? as usize
        } else {
            pos + 8
        };
        let data = source
            .get(data_offset..data_offset.checked_add(size).ok_or("EXIF overflow")?)
            .ok_or("Truncated EXIF value")?;
        entries.push((source[pos..pos + 8].to_vec(), data.to_vec(), pointer));
    }
    let base = output.len();
    let count = entries.len() as u16;
    output.extend(if little {
        count.to_le_bytes()
    } else {
        count.to_be_bytes()
    });
    output.resize(base + 2 + entries.len() * 12 + 4, 0);
    for (index, (header, data, pointer)) in entries.into_iter().enumerate() {
        let pos = base + 2 + index * 12;
        output[pos..pos + 8].copy_from_slice(&header);
        if pointer || data.len() > 4 {
            let target = output.len() as u32;
            output[pos + 8..pos + 12].copy_from_slice(&if little {
                target.to_le_bytes()
            } else {
                target.to_be_bytes()
            });
            if pointer {
                copy_ifd(
                    source,
                    number(&data, 0, 4, little)? as usize,
                    level + 1,
                    little,
                    output,
                )?;
            } else {
                output.extend(data);
            }
        } else {
            output[pos + 8..pos + 8 + data.len()].copy_from_slice(&data);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn private_exif(little: bool, orientation: u16) -> Vec<u8> {
        let mut tiff = if little {
            b"II\x2a\0".to_vec()
        } else {
            b"MM\0\x2a".to_vec()
        };
        put32(&mut tiff, 8, little);
        let put16 = |out: &mut Vec<u8>, value: u16| {
            out.extend(if little {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            })
        };
        put16(&mut tiff, 3);
        // Orientation, camera model, GPS pointer. Private payloads must not survive.
        for (tag, kind, count, value) in [(0x112, 3, 1, 0), (0x110, 2, 15, 50), (0x8825, 4, 1, 65)]
        {
            put16(&mut tiff, tag);
            put16(&mut tiff, kind);
            put32(&mut tiff, count, little);
            if tag == 0x112 {
                put16(&mut tiff, orientation);
                put16(&mut tiff, 0);
            } else {
                put32(&mut tiff, value, little);
            }
        }
        put32(&mut tiff, 0, little);
        tiff.extend(b"private-camera\0private-location");
        tiff
    }

    #[test]
    fn tiff_rebuild_removes_private_data_for_both_byte_orders_and_all_orientations() {
        for little in [false, true] {
            for orientation in 1..=8 {
                let clean = exif(&private_exif(little, orientation)).unwrap();
                assert_eq!(clean.len(), 26);
                assert_eq!(
                    number(&clean, 18, 2, little).unwrap(),
                    u32::from(orientation)
                );
                assert_eq!(exif(&clean).unwrap(), clean);
            }
        }
        assert!(exif(&private_exif(true, 9)).is_err());
        for len in 0..50 {
            assert!(exif(&private_exif(true, 6)[..len]).is_err());
        }
    }

    #[test]
    fn private_metadata_is_removed_and_rendering_is_preserved_by_real_engines() {
        let dir = tempfile::tempdir().unwrap();
        for png in [false, true] {
            let path = dir.path().join(if png { "test.png" } else { "test.jpg" });
            super::super::tests::fixture(&path, png, 257, 131);
            let source = fs::read(&path).unwrap();
            let tiff = private_exif(true, 6);
            let marked = if png {
                let mut bytes = source[..33].to_vec();
                bytes.extend(png_chunk(*b"eXIf", &tiff));
                bytes.extend(png_chunk(*b"tEXt", b"Author\0private-comment"));
                bytes.extend(png_chunk(*b"gAMA", &45455u32.to_be_bytes()));
                bytes.extend(&source[33..]);
                bytes
            } else {
                let mut bytes = source[..2].to_vec();
                for (kind, data) in [
                    (0xe1, [b"Exif\0\0".as_slice(), &tiff].concat()),
                    (0xfe, b"private-comment".to_vec()),
                ] {
                    bytes.extend([255, kind]);
                    bytes.extend(((data.len() + 2) as u16).to_be_bytes());
                    bytes.extend(data);
                }
                bytes.extend(&source[2..]);
                bytes
            };
            fs::write(&path, &marked).unwrap();
            let (before, after) =
                optimize(&path, &super::super::tests::engine(), &Mutex::new(false), 2).unwrap();
            assert!(after < before);
            let output = fs::read(&path).unwrap();
            assert!(!output.windows(7).any(|w| w == b"private"));
            assert!(same_pixels(
                &decode(&marked).unwrap(),
                &decode(&output).unwrap()
            ));
            if png {
                assert_eq!(
                    png_expected(&marked).unwrap(),
                    png_metadata(&output).unwrap()
                );
            } else {
                assert_eq!(
                    jpeg_expected(&marked).unwrap(),
                    jpeg_metadata(&output).unwrap()
                );
            }
        }
    }

    #[test]
    fn malformed_exif_and_unknown_rendering_metadata_leave_original_intact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.jpg");
        super::super::tests::fixture(&path, false, 100, 100);
        let original = fs::read(&path).unwrap();
        for data in [
            b"Exif\0\0broken".as_slice(),
            b"http://ns.adobe.com/xap/1.0/\0hdrgm:Version=1",
        ] {
            let mut bytes = original[..2].to_vec();
            bytes.extend([255, 0xe1]);
            bytes.extend(((data.len() + 2) as u16).to_be_bytes());
            bytes.extend(data);
            bytes.extend(&original[2..]);
            fs::write(&path, &bytes).unwrap();
            assert!(
                optimize(&path, &super::super::tests::engine(), &Mutex::new(false), 2).is_err()
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
}

// Strip XMP only when every property is known to be descriptive. Namespace-aware
// parsing avoids treating an HDR/custom property with a familiar prefix as safe.
fn disposable_xmp(packet: &[u8]) -> Result<()> {
    use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
    fn allowed(ns: ResolveResult<'_>, local: &str) -> bool {
        let ResolveResult::Bound(ns) = ns else {
            return false;
        };
        match ns.as_ref() {
            "adobe:ns:meta/" => matches!(local, "xmpmeta" | "xmptk"),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#" => matches!(
                local,
                "RDF" | "Description" | "about" | "Bag" | "Seq" | "Alt" | "li" | "parseType"
            ),
            "http://www.w3.org/XML/1998/namespace" => local == "lang",
            "http://purl.org/dc/elements/1.1/" => matches!(
                local,
                "creator" | "description" | "title" | "rights" | "subject" | "format"
            ),
            "http://ns.adobe.com/xap/1.0/" => matches!(
                local,
                "CreateDate" | "ModifyDate" | "MetadataDate" | "CreatorTool" | "Rating" | "Label"
            ),
            "http://ns.adobe.com/exif/1.0/" => matches!(
                local,
                "PixelXDimension"
                    | "PixelYDimension"
                    | "UserComment"
                    | "DateTimeOriginal"
                    | "DateTimeDigitized"
                    | "GPSLatitude"
                    | "GPSLongitude"
                    | "GPSAltitude"
            ),
            "http://ns.adobe.com/tiff/1.0/" => matches!(
                local,
                "Make"
                    | "Model"
                    | "Software"
                    | "DateTime"
                    | "Artist"
                    | "Copyright"
                    | "ImageDescription"
            ),
            _ => false,
        }
    }
    let mut reader = NsReader::from_reader(packet);
    let mut depth = 0usize;
    let mut roots = 0usize;
    loop {
        let (ns, event) = reader
            .read_resolved_event()
            .map_err(|_| "Invalid XMP XML; original retained")?;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                if !allowed(ns, tag.local_name().as_ref()) {
                    return Err(unsupported_xmp());
                }
                for attribute in tag.attributes() {
                    let attribute =
                        attribute.map_err(|_| "Invalid XMP attribute; original retained")?;
                    if attribute.key.as_ref() == "xmlns"
                        || attribute.key.as_ref().starts_with("xmlns:")
                    {
                        continue;
                    }
                    let (ns, local) = reader.resolver().resolve_attribute(attribute.key);
                    if !allowed(ns, local.as_ref()) {
                        return Err(unsupported_xmp());
                    }
                }
                if depth == 0 {
                    roots += 1;
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                }
                if depth > 32 {
                    return Err(unsupported_xmp());
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or("Invalid XMP nesting")?;
            }
            Event::DocType(_) | Event::GeneralRef(_) => return Err(unsupported_xmp()),
            Event::Eof => break,
            _ => (),
        }
    }
    if depth != 0 || roots != 1 {
        return Err("Invalid XMP document".into());
    }
    Ok(())
}

fn png_xmp(payload: &[u8]) -> Result<()> {
    let tail = payload
        .splitn(2, |b| *b == 0)
        .nth(1)
        .ok_or("Invalid XMP chunk")?;
    if !tail.starts_with(&[0, 0]) {
        return Err(unsupported_xmp());
    }
    let tail = tail[2..]
        .splitn(2, |b| *b == 0)
        .nth(1)
        .ok_or("Invalid XMP language")?;
    let text = tail
        .splitn(2, |b| *b == 0)
        .nth(1)
        .ok_or("Invalid XMP translation")?;
    disposable_xmp(text)
}

#[cfg(test)]
mod xmp_tests {
    use super::*;
    const PRIVATE: &[u8] = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:exif="http://ns.adobe.com/exif/1.0/" rdf:about=""><exif:UserComment>private comment</exif:UserComment><exif:PixelXDimension>640</exif:PixelXDimension></rdf:Description></rdf:RDF></x:xmpmeta>"#;
    #[test]
    fn descriptive_xmp_is_removed_but_rendering_unknown_and_entities_are_rejected() {
        disposable_xmp(PRIVATE).unwrap();
        let error = disposable_xmp(
            br#"<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:xml="private-namespace-value"/>"#,
        )
        .unwrap_err()
        .to_string();
        assert!(!error.contains("private-namespace-value"));
        for packet in [
            String::from_utf8(PRIVATE.to_vec())
                .unwrap()
                .replace("UserComment", "ColorSpace"),
            String::from_utf8(PRIVATE.to_vec())
                .unwrap()
                .replace("http://ns.adobe.com/exif/1.0/", "urn:hdr:gainmap"),
            String::from_utf8(PRIVATE.to_vec())
                .unwrap()
                .replace("private comment", "&external;"),
            String::from_utf8(PRIVATE.to_vec())
                .unwrap()
                .replace("</rdf:RDF>", ""),
        ] {
            assert!(disposable_xmp(packet.as_bytes()).is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.jpg");
        super::super::tests::fixture(&path, false, 257, 131);
        let source = fs::read(&path).unwrap();
        let payload = [b"http://ns.adobe.com/xap/1.0/\0".as_slice(), PRIVATE].concat();
        let mut bytes = source[..2].to_vec();
        bytes.extend([255, 0xe1]);
        bytes.extend(((payload.len() + 2) as u16).to_be_bytes());
        bytes.extend(payload);
        bytes.extend(&source[2..]);
        fs::write(&path, &bytes).unwrap();
        optimize(&path, &super::super::tests::engine(), &Mutex::new(false), 2).unwrap();
        let output = fs::read(&path).unwrap();
        assert!(!output.windows(7).any(|w| w == b"private"));
        assert!(same_pixels(
            &decode(&bytes).unwrap(),
            &decode(&output).unwrap()
        ));
    }
}
