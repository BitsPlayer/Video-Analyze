//! File-backed ISOBMFF box walk. Reads `moov` / `moof` payloads and never touches `mdat` media bytes.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::domain::boxes::{TopLevelBox, fourcc};

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
