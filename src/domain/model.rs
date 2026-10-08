//! Layout model: spans, track info, and the analysis result.

use std::path::PathBuf;

use super::human_bytes;

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
    pub path: PathBuf,
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
