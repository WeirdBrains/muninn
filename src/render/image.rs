//! Image facts without decoding the whole image: size from PNG, JPEG, GIF and BMP headers.

/// Width and height in pixels, when the format is known.
pub fn size(data: &[u8]) -> Option<(u32, u32)> {
    let be32 = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    if data.len() > 24 && data.starts_with(b"\x89PNG\r\n\x1a\n") && &data[12..16] == b"IHDR" {
        return Some((be32(&data[16..20]), be32(&data[20..24])));
    }
    if data.len() > 10 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        return Some((u16::from_le_bytes([data[6], data[7]]) as u32, u16::from_le_bytes([data[8], data[9]]) as u32));
    }
    if data.len() > 26 && data.starts_with(b"BM") {
        let w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
        let h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
        return Some((w.unsigned_abs(), h.unsigned_abs()));
    }
    jpeg(data).map(|j| (j.0, j.1))
}

/// JPEG width, height and color components (from the SOF marker).
pub fn jpeg(data: &[u8]) -> Option<(u32, u32, u8)> {
    if !data.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = data[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        if m == 0xD8 || (0xD0..=0xD7).contains(&m) || m == 0x01 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            return Some((w, h, data[i + 9]));
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(size(&super::super::sample::png(8, 4)), Some((8, 4)));
        let gif = b"GIF89a\x0a\x00\x05\x00rest";
        assert_eq!(size(gif), Some((10, 5)));
        // SOI, APP0 (len 4), SOF0 h=3 w=7 3 components
        let jpg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0, 0, 0xFF, 0xC0, 0x00, 0x11, 8, 0, 3, 0, 7, 3, 0, 0, 0, 0, 0, 0];
        assert_eq!(jpeg(&jpg), Some((7, 3, 3)));
        assert_eq!(size(b"nope"), None);
    }
}
