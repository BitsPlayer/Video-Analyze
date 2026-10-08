//! Application layer: analysis use case. Depends only on the domain and the box-source port.

use std::path::Path;

use anyhow::Result;

use crate::domain::boxes::TopLevelBox;
use crate::domain::{self, Analysis};

pub trait BoxSource {
    fn walk_file(&self, path: &Path) -> Result<(u64, Vec<TopLevelBox>)>;
}

pub fn analyze(path: &Path, source: &impl BoxSource) -> Result<Analysis> {
    let (file_size, tops) = source.walk_file(path)?;
    let parsed = domain::build(file_size, &tops);
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
