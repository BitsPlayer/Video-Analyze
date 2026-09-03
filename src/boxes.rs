//! ISOBMFF box walking. Reads `moov` / `moof` payloads and never touches `mdat` media bytes.

use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct RawBox<'a> {
    pub file_offset: u64,
    pub header_len: u64,
    #[allow(dead_code)]
    pub size: u64,
    pub typ: [u8; 4],
    pub payload: &'a [u8],
}

#[allow(dead_code)]
impl RawBox<'_> {
    pub fn fourcc(&self) -> String {
        fourcc(&self.typ)
    }

    pub fn is_type(&self, t: &[u8; 4]) -> bool {
        &self.typ == t
    }

    pub fn children(&self) -> Vec<RawBox<'_>> {
        iter_boxes(self.payload, self.file_offset + self.header_len)
    }

    pub fn child(&self, t: &[u8; 4]) -> Option<RawBox<'_>> {
        self.children().into_iter().find(|b| b.is_type(t))
    }

    pub fn descendants<'a>(&'a self, t: &[u8; 4]) -> Vec<RawBox<'a>> {
        let mut out = Vec::new();
        collect_type(
            self.payload,
            self.file_offset + self.header_len,
            t,
            &mut out,
        );
        out
    }

    pub fn fullbox_payload(&self) -> Option<(u8, u32, &[u8])> {
        if self.payload.len() < 4 {
            return None;
        }
        let version = self.payload[0];
        let flags = u32::from_be_bytes([0, self.payload[1], self.payload[2], self.payload[3]]);
        Some((version, flags, &self.payload[4..]))
    }
}

#[derive(Debug, Clone)]
pub struct TopLevelBox {
    pub offset: u64,
    pub size: u64,
    pub header_len: u64,
    pub typ: [u8; 4],
    pub payload: Option<Vec<u8>>,
}

#[allow(dead_code)]
impl TopLevelBox {
    pub fn fourcc(&self) -> String {
        fourcc(&self.typ)
    }

    pub fn is_type(&self, t: &[u8; 4]) -> bool {
        &self.typ == t
    }
}

pub fn fourcc(t: &[u8; 4]) -> String {
    t.iter()
        .map(|&b| {
            if (0x20..=0x7e).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect()
}

pub fn iter_boxes(data: &[u8], file_offset: u64) -> Vec<RawBox<'_>> {
    let mut out = Vec::new();
    let mut off = 0usize;
    let len = data.len();
    while off + 8 <= len {
        match parse_header(&data[off..], (len - off) as u64) {
            Ok((header_len, box_size, typ)) => {
                let payload_start = off + header_len as usize;
                let end = (off as u64).saturating_add(box_size).min(len as u64) as usize;
                if end < payload_start {
                    break;
                }
                out.push(RawBox {
                    file_offset: file_offset + off as u64,
                    header_len,
                    size: box_size,
                    typ,
                    payload: &data[payload_start..end],
                });
                let next = off as u64 + box_size;
                if next <= off as u64 {
                    break;
                }
                off = next as usize;
            }
            Err(_) => break,
        }
    }
    out
}

fn collect_type<'a>(data: &'a [u8], file_offset: u64, t: &[u8; 4], out: &mut Vec<RawBox<'a>>) {
    for b in iter_boxes(data, file_offset) {
        if b.is_type(t) {
            out.push(b);
        }
        collect_type(b.payload, b.file_offset + b.header_len, t, out);
    }
}

/// Parses an ISOBMFF box header at the start of `slice`. `remaining` is bytes from this
/// offset to the end of the enclosing buffer (file or parent payload).
pub fn parse_header(slice: &[u8], remaining: u64) -> Result<(u64, u64, [u8; 4])> {
    if slice.len() < 8 {
        bail!("truncated box header");
    }
    let size32 = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as u64;
    let typ = [slice[4], slice[5], slice[6], slice[7]];
    let (header_len, box_size) = match size32 {
        0 => (8u64, remaining),
        1 => {
            if slice.len() < 16 {
                bail!("truncated 64-bit box header");
            }
            let large = u64::from_be_bytes(slice[8..16].try_into().unwrap());
            (16u64, large)
        }
        n if n < 8 => bail!("invalid box size {n} for type {}", fourcc(&typ)),
        n => (8u64, n),
    };
    if box_size < header_len {
        bail!(
            "box size {box_size} smaller than header {header_len} for {}",
            fourcc(&typ)
        );
    }
    Ok((header_len, box_size, typ))
}

const PAYLOAD_TYPES: [[u8; 4]; 2] = [*b"moov", *b"moof"];

/// Walks top-level boxes. `mdat` payload is never read; `moov`/`moof` payloads are.
pub fn walk_file(path: &Path) -> Result<(u64, Vec<TopLevelBox>)> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let file_size = file.metadata()?.len();
    let mut boxes = Vec::new();
    let mut off = 0u64;

    while off + 8 <= file_size {
        file.seek(SeekFrom::Start(off))?;
        let mut hdr8 = [0u8; 8];
        file.read_exact(&mut hdr8)
            .with_context(|| format!("reading box header at {off}"))?;
        let remaining = file_size - off;
        let size32 = u32::from_be_bytes(hdr8[0..4].try_into().unwrap()) as u64;
        let typ = [hdr8[4], hdr8[5], hdr8[6], hdr8[7]];
        let (header_len, box_size) = match size32 {
            0 => (8u64, remaining),
            1 => {
                if remaining < 16 {
                    break;
                }
                let mut large = [0u8; 8];
                file.read_exact(&mut large)?;
                (16u64, u64::from_be_bytes(large))
            }
            n if n < 8 => break,
            n => (8u64, n),
        };
        if box_size < header_len {
            break;
        }

        let mut payload = None;
        if PAYLOAD_TYPES.contains(&typ) {
            let payload_len = box_size.saturating_sub(header_len);
            let readable = payload_len.min(file_size.saturating_sub(off + header_len));
            if readable > 0 {
                file.seek(SeekFrom::Start(off + header_len))?;
                let mut buf = vec![0u8; readable as usize];
                file.read_exact(&mut buf)
                    .with_context(|| format!("reading {} payload at {off}", fourcc(&typ)))?;
                payload = Some(buf);
            } else {
                payload = Some(Vec::new());
            }
        }

        boxes.push(TopLevelBox {
            offset: off,
            size: box_size.min(remaining),
            header_len,
            typ,
            payload,
        });

        let next = off.saturating_add(box_size);
        if next <= off || next > file_size {
            break;
        }
        off = next;
    }

    Ok((file_size, boxes))
}

pub fn read_u32(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_be_bytes)
}

pub fn read_u64(data: &[u8], at: usize) -> Option<u64> {
    data.get(at..at + 8)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_be_bytes)
}

pub fn read_i32(data: &[u8], at: usize) -> Option<i32> {
    data.get(at..at + 4)
        .and_then(|s| s.try_into().ok())
        .map(i32::from_be_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_header() {
        let mut b = vec![0, 0, 0, 16];
        b.extend_from_slice(b"ftyp");
        b.extend_from_slice(&[0; 8]);
        let (h, size, typ) = parse_header(&b, 16).unwrap();
        assert_eq!(h, 8);
        assert_eq!(size, 16);
        assert_eq!(&typ, b"ftyp");
    }

    #[test]
    fn size_zero_extends_to_end() {
        let mut b = vec![0, 0, 0, 0];
        b.extend_from_slice(b"mdat");
        let (h, size, typ) = parse_header(&b, 100).unwrap();
        assert_eq!(h, 8);
        assert_eq!(size, 100);
        assert_eq!(&typ, b"mdat");
    }
}
