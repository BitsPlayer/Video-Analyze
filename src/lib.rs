//! MP4/MOV layout analysis: top-level boxes, sample tables, GOP structure.
//!
//! Media bytes in `mdat` are never read. A truncated file without `moov` still
//! yields a top-level box map so the GUI can point the user at extraction.

pub mod application;
pub mod domain;
pub mod infrastructure;

use std::path::Path;

use anyhow::Result;

pub use domain::boxes::fourcc;
pub use domain::{Analysis, AudioInfo, LayoutSpan, SpanKind, VideoInfo, human_bytes};

pub fn analyze(path: &Path) -> Result<Analysis> {
    application::analyze(path, &infrastructure::FileBoxSource)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn truncated_file_without_moov_still_maps() {
        let dir = std::env::temp_dir().join("bitplay-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("truncated.mp4");
        let mut data = Vec::new();
        data.extend_from_slice(&20u32.to_be_bytes());
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(b"isom");
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(b"isom");
        data.extend_from_slice(&72u32.to_be_bytes());
        data.extend_from_slice(b"mdat");
        data.extend_from_slice(&[0u8; 64]);
        std::fs::File::create(&path)
            .unwrap()
            .write_all(&data)
            .unwrap();

        let analysis = analyze(&path).unwrap();
        assert!(!analysis.moov_present);
        assert_eq!(analysis.file_size, data.len() as u64);
        let kinds: Vec<_> = analysis.spans.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&SpanKind::Ftyp), "missing ftyp in {kinds:?}");
        assert!(
            kinds.contains(&SpanKind::MdatHeader),
            "missing mdat header in {kinds:?}"
        );
    }
}
