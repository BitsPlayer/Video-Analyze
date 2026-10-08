//! Domain layer: in-memory MP4 layout model. No filesystem access.

mod format;
mod layout;
pub(crate) mod boxes;
mod model;

pub use format::human_bytes;
pub use layout::build;
pub use model::{Analysis, AudioInfo, LayoutSpan, SpanKind, VideoInfo};
