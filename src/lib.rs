//! MP4 layout analysis: top-level boxes, sample tables, GOP structure.
//!
//! Media bytes in `mdat` are never read. A truncated file without `moov` still
//! yields a top-level box map so the GUI can point the user at extraction.

mod boxes;
mod layout;

use std::path::Path;

use anyhow::Result;

pub use boxes::fourcc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Ftyp,
    Moov,
    MdatHeader,
    Free,
    Unknown,
    VideoI { gop: u32 },
    VideoP { gop: u32 },
    VideoB { gop: u32 },
    Audio,
}

impl SpanKind {
    pub fn gop(self) -> Option<u32> {
        match self {
            SpanKind::VideoI { gop } | SpanKind::VideoP { gop } | SpanKind::VideoB { gop } => {
                Some(gop)
            }
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SpanKind::Ftyp => "ftyp",
            SpanKind::Moov => "moov",
            SpanKind::MdatHeader => "mdat header",
            SpanKind::Free => "free",
            SpanKind::Unknown => "unmapped",
            SpanKind::VideoI { .. } => "I-frame",
            SpanKind::VideoP { .. } => "P-frame",
            SpanKind::VideoB { .. } => "B-frame",
            SpanKind::Audio => "audio",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutSpan {
    pub offset: u64,
    pub size: u64,
    pub kind: SpanKind,
}

impl LayoutSpan {
    pub fn contains(&self, byte: u64) -> bool {
        byte >= self.offset && byte < self.offset.saturating_add(self.size)
    }

    pub fn status_line(&self) -> String {
        let size = human_bytes(self.size);
        match self.kind.gop() {
            Some(gop) => format!("{} · GOP {gop} · {size}", self.kind.label()),
            None => format!("{} · {size}", self.kind.label()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub track_id: u32,
    pub width: u32,
    pub height: u32,
    pub codec: String,
    pub sample_count: u32,
    pub iframe_count: u32,
    pub gop_count: u32,
    pub fps: Option<f64>,
    pub profile: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AudioInfo {
    pub track_id: u32,
    pub codec: String,
    pub sample_count: u32,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub path: std::path::PathBuf,
    pub file_size: u64,
    pub duration_secs: Option<f64>,
    pub timescale: Option<u32>,
    pub video: Option<VideoInfo>,
    pub audio: Option<AudioInfo>,
    pub spans: Vec<LayoutSpan>,
    pub moov_present: bool,
}

impl Analysis {
    pub fn span_at(&self, offset: u64) -> Option<&LayoutSpan> {
        let i = self
            .spans
            .partition_point(|s| s.offset.saturating_add(s.size) <= offset);
        self.spans.get(i).filter(|s| s.contains(offset))
    }
}

pub fn analyze(path: &Path) -> Result<Analysis> {
    let (file_size, tops) = boxes::walk_file(path)?;
    let parsed = layout::build(file_size, &tops);
    Ok(Analysis {
        path: path.to_path_buf(),
        file_size,
        duration_secs: parsed.duration_secs,
        timescale: parsed.timescale,
        video: parsed.video,
        audio: parsed.audio,
        spans: parsed.spans,
        moov_present: parsed.moov_present,
    })
}

pub fn human_bytes(n: u64) -> String {
    const K: f64 = 1024.0;
    let x = n as f64;
    if x < K {
        format!("{n} B")
    } else if x < K * K {
        format!("{:.1} KiB", x / K)
    } else if x < K * K * K {
        format!("{:.1} MiB", x / (K * K))
    } else {
        format!("{:.2} GiB", x / (K * K * K))
    }
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

    #[test]
    fn human_bytes_formats() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KiB");
    }
}
