//! Infrastructure layer: read top-level boxes from a file. Implements the application port.

mod walk;

use std::path::Path;

use anyhow::Result;

use crate::application::BoxSource;
use crate::domain::boxes::TopLevelBox;

pub struct FileBoxSource;

impl BoxSource for FileBoxSource {
    fn walk_file(&self, path: &Path) -> Result<(u64, Vec<TopLevelBox>)> {
        walk::walk_file(path)
    }
}
