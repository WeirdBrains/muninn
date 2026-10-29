//! A minimal ZIP writer (deflate) for Docx, and a reader for checking what was written.
use flate2::Compression;
use flate2::write::DeflateEncoder;
use std::io::Write;

/// Files to zip, in order.
#[derive(Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

impl ZipWriter {
    pub fn add(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
        e.write_all(data)?;
        let comp = e.finish()?;
        let crc = crc32fast::hash(data);
        let off = self.out.len() as u32;
        // fixed timestamp (1980-01-01 00:00) keeps output reproducible
        let (time, date) = (0u16, 0x21u16);
        let n = name.as_bytes();
        let mut h = vec![];
        h.extend_from_slice(&0x04034b50u32.to_le_bytes());
        h.extend_from_slice(&20u16.to_le_bytes()); // version needed
        h.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
        h.extend_from_slice(&8u16.to_le_bytes()); // deflate
        h.extend_from_slice(&time.to_le_bytes());
        h.extend_from_slice(&date.to_le_bytes());
        h.extend_from_slice(&crc.to_le_bytes());
        h.extend_from_slice(&(comp.len() as u32).to_le_bytes());
        h.extend_from_slice(&(data.len() as u32).to_le_bytes());
        h.extend_from_slice(&(n.len() as u16).to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(n);
        self.out.extend_from_slice(&h);
        self.out.extend_from_slice(&comp);
        let c = &mut self.central;
        c.extend_from_slice(&0x02014b50u32.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes()); // made by
        c.extend_from_slice(&20u16.to_le_bytes());
        c.extend_from_slice(&0x0800u16.to_le_bytes());
        c.extend_from_slice(&8u16.to_le_bytes());
        c.extend_from_slice(&time.to_le_bytes());
        c.extend_from_slice(&date.to_le_bytes());
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&(comp.len() as u32).to_le_bytes());
        c.extend_from_slice(&(data.len() as u32).to_le_bytes());
        c.extend_from_slice(&(n.len() as u16).to_le_bytes());
        c.extend_from_slice(&[0; 8]); // extra, comment, disk, internal attrs
        c.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        c.extend_from_slice(&off.to_le_bytes());
        c.extend_from_slice(n);
        self.count += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Vec<u8> {
        let off = self.out.len() as u32;
        let size = self.central.len() as u32;
        self.out.extend_from_slice(&self.central);
        self.out.extend_from_slice(&0x06054b50u32.to_le_bytes());
        self.out.extend_from_slice(&[0; 4]);
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&size.to_le_bytes());
        self.out.extend_from_slice(&off.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out
    }
}

/// Read every file of a ZIP written by [`ZipWriter`] (deflate or stored entries).
pub fn read(data: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let le16 = |i: usize| -> Option<usize> { Some(u16::from_le_bytes([*data.get(i)?, *data.get(i + 1)?]) as usize) };
    let le32 = |i: usize| -> Option<usize> { Some(u32::from_le_bytes(data.get(i..i + 4)?.try_into().ok()?) as usize) };
    let eocd = (0..data.len().saturating_sub(21)).rev().find(|&i| data[i..].starts_with(&0x06054b50u32.to_le_bytes()))?;
    let n = le16(eocd + 10)?;
    let mut p = le32(eocd + 16)?;
    let mut out = vec![];
    for _ in 0..n {
        if le32(p)? != 0x02014b50 {
            return None;
        }
        let method = le16(p + 10)?;
        let csize = le32(p + 20)?;
        let nlen = le16(p + 28)?;
        let extra = le16(p + 30)?;
        let comment = le16(p + 32)?;
        let local = le32(p + 42)?;
        let name = String::from_utf8_lossy(data.get(p + 46..p + 46 + nlen)?).to_string();
        let lstart = local + 30 + le16(local + 26)? + le16(local + 28)?;
        let raw = data.get(lstart..lstart + csize)?;
        let body = match method {
            0 => raw.to_vec(),
            8 => {
                let mut d = flate2::read::DeflateDecoder::new(raw);
                let mut v = vec![];
                std::io::Read::read_to_end(&mut d, &mut v).ok()?;
                v
            }
            _ => return None,
        };
        out.push((name, body));
        p += 46 + nlen + extra + comment;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_round_trip() {
        let mut z = ZipWriter::default();
        z.add("a.txt", b"hello hello hello").unwrap();
        z.add("dir/b.bin", &[0u8; 1000]).unwrap();
        let bytes = z.finish();
        assert!(bytes.starts_with(b"PK\x03\x04"));
        let files = read(&bytes).unwrap();
        assert_eq!(files[0], ("a.txt".to_string(), b"hello hello hello".to_vec()));
        assert_eq!(files[1].1.len(), 1000);
    }
}
