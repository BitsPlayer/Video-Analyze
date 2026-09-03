//! Turns top-level boxes + sample tables into a byte-accurate file layout.

use crate::boxes::{self, RawBox, TopLevelBox, fourcc, read_i32, read_u32, read_u64};
use crate::{AudioInfo, LayoutSpan, SpanKind, VideoInfo};

pub struct ParsedFile {
    pub duration_secs: Option<f64>,
    pub timescale: Option<u32>,
    pub video: Option<VideoInfo>,
    pub audio: Option<AudioInfo>,
    pub spans: Vec<LayoutSpan>,
    pub moov_present: bool,
}

struct Sample {
    offset: u64,
    size: u64,
    is_sync: bool,
    is_b: bool,
}

struct TrackLayout {
    is_video: bool,
    info_video: Option<VideoInfo>,
    info_audio: Option<AudioInfo>,
    samples: Vec<Sample>,
}

pub fn build(file_size: u64, tops: &[TopLevelBox]) -> ParsedFile {
    let mut spans = Vec::new();
    let mut moov_payload: Option<(u64, &[u8])> = None;
    let mut moofs: Vec<&TopLevelBox> = Vec::new();

    for b in tops {
        let kind = match &b.typ {
            b"ftyp" => SpanKind::Ftyp,
            b"moov" => SpanKind::Moov,
            b"free" | b"skip" | b"wide" => SpanKind::Free,
            b"mdat" => SpanKind::MdatHeader,
            _ => SpanKind::Unknown,
        };
        if b.is_type(b"mdat") {
            spans.push(LayoutSpan {
                offset: b.offset,
                size: b.header_len.min(b.size),
                kind: SpanKind::MdatHeader,
            });
        } else {
            spans.push(LayoutSpan {
                offset: b.offset,
                size: b.size.min(file_size.saturating_sub(b.offset)),
                kind,
            });
        }
        if b.is_type(b"moov") {
            if let Some(p) = b.payload.as_deref() {
                moov_payload = Some((b.offset + b.header_len, p));
            }
        }
        if b.is_type(b"moof") {
            moofs.push(b);
        }
    }

    let moov_present = moov_payload.is_some();
    let mut duration_secs = None;
    let mut timescale = None;
    let mut video = None;
    let mut audio = None;

    if let Some((base, payload)) = moov_payload {
        let moov_children = boxes::iter_boxes(payload, base);

        if let Some(mvhd) = moov_children.iter().find(|c| c.is_type(b"mvhd")) {
            if let Some((ts, dur)) = parse_mvhd(*mvhd) {
                timescale = Some(ts);
                if ts > 0 {
                    duration_secs = Some(dur as f64 / ts as f64);
                }
            }
        }

        let mut tracks = Vec::new();
        for trak in moov_children.iter().filter(|c| c.is_type(b"trak")) {
            if let Some(t) = parse_trak(*trak) {
                tracks.push(t);
            }
        }

        if !moofs.is_empty() {
            apply_fragments(&mut tracks, &moofs);
        }

        let mut gop = 0u32;
        for t in &mut tracks {
            if t.is_video {
                for s in &t.samples {
                    if s.is_sync {
                        gop += 1;
                    }
                    let kind = if s.is_sync {
                        SpanKind::VideoI {
                            gop: gop.saturating_sub(1),
                        }
                    } else if s.is_b {
                        SpanKind::VideoB {
                            gop: gop.saturating_sub(1),
                        }
                    } else {
                        SpanKind::VideoP {
                            gop: gop.saturating_sub(1),
                        }
                    };
                    spans.push(LayoutSpan {
                        offset: s.offset,
                        size: s.size,
                        kind,
                    });
                }
                if let Some(v) = t.info_video.as_mut() {
                    v.iframe_count = t.samples.iter().filter(|s| s.is_sync).count() as u32;
                    v.gop_count = gop;
                    v.sample_count = t.samples.len() as u32;
                    video = Some(v.clone());
                }
            } else {
                for s in &t.samples {
                    spans.push(LayoutSpan {
                        offset: s.offset,
                        size: s.size,
                        kind: SpanKind::Audio,
                    });
                }
                if let Some(a) = t.info_audio.as_mut() {
                    a.sample_count = t.samples.len() as u32;
                    audio = Some(a.clone());
                }
            }
        }
    }

    ParsedFile {
        duration_secs,
        timescale,
        video,
        audio,
        spans: normalize(spans, file_size),
        moov_present,
    }
}

fn normalize(mut spans: Vec<LayoutSpan>, file_size: u64) -> Vec<LayoutSpan> {
    spans.sort_by_key(|s| (s.offset, s.size));
    let mut out = Vec::new();
    let mut cursor = 0u64;
    for s in spans {
        if s.size == 0 {
            continue;
        }
        if s.offset > cursor && s.offset < file_size {
            out.push(LayoutSpan {
                offset: cursor,
                size: s.offset - cursor,
                kind: SpanKind::Unknown,
            });
            cursor = s.offset;
        }
        if s.offset < cursor {
            continue;
        }
        let end = s.offset.saturating_add(s.size).min(file_size);
        if end > s.offset {
            out.push(LayoutSpan {
                offset: s.offset,
                size: end - s.offset,
                kind: s.kind,
            });
            cursor = end;
        }
    }
    if cursor < file_size {
        out.push(LayoutSpan {
            offset: cursor,
            size: file_size - cursor,
            kind: SpanKind::Unknown,
        });
    }
    out
}

fn parse_mvhd(b: RawBox<'_>) -> Option<(u32, u64)> {
    let (version, _, data) = b.fullbox_payload()?;
    if version == 1 {
        let timescale = read_u32(data, 16)?;
        let duration = read_u64(data, 20)?;
        Some((timescale, duration))
    } else {
        let timescale = read_u32(data, 8)?;
        let duration = read_u32(data, 12)? as u64;
        Some((timescale, duration))
    }
}

fn parse_trak(trak: RawBox<'_>) -> Option<TrackLayout> {
    let tkhd = trak.child(&b"tkhd")?;
    let (track_id, width, height) = parse_tkhd(tkhd)?;
    let mdia = trak.child(&b"mdia")?;
    let handler = mdia.child(&b"hdlr").and_then(parse_hdlr).unwrap_or([0; 4]);
    let is_video = &handler == b"vide";
    let is_audio = &handler == b"soun";
    if !is_video && !is_audio {
        return None;
    }

    let mdhd = mdia.child(&b"mdhd");
    let media_timescale = mdhd.and_then(parse_mdhd_timescale);

    let minf = mdia.child(&b"minf")?;
    let stbl = minf.child(&b"stbl")?;

    let (codec, profile, stsd_wh) = stbl
        .child(&b"stsd")
        .map(parse_stsd)
        .unwrap_or_else(|| ("unknown".into(), None, (0, 0)));
    let (vw, vh) = if width > 0 && height > 0 {
        (width, height)
    } else {
        stsd_wh
    };

    let samples = parse_stbl_samples(&stbl, is_video);
    let fps = media_timescale.and_then(|ts| {
        stbl.child(&b"stts")
            .and_then(parse_stts_fps)
            .map(|delta| ts as f64 / delta as f64)
    });

    let mut layout = TrackLayout {
        is_video,
        info_video: None,
        info_audio: None,
        samples,
    };

    if is_video {
        layout.info_video = Some(VideoInfo {
            track_id,
            width: vw,
            height: vh,
            codec,
            sample_count: 0,
            iframe_count: 0,
            gop_count: 0,
            fps,
            profile,
        });
    } else {
        layout.info_audio = Some(AudioInfo {
            track_id,
            codec,
            sample_count: 0,
        });
    }
    Some(layout)
}

fn parse_tkhd(b: RawBox<'_>) -> Option<(u32, u32, u32)> {
    let (version, _, data) = b.fullbox_payload()?;
    let (track_id, rest) = if version == 1 {
        (read_u32(data, 16)?, data.get(32..)?)
    } else {
        (read_u32(data, 8)?, data.get(20..)?)
    };
    // reserved 8 + layer 2 + alt 2 + volume 2 + reserved 2 + matrix 36 = 52
    let width = read_u32(rest, 52)? >> 16;
    let height = read_u32(rest, 56)? >> 16;
    Some((track_id, width, height))
}

fn parse_hdlr(b: RawBox<'_>) -> Option<[u8; 4]> {
    let (_, _, data) = b.fullbox_payload()?;
    data.get(4..8)?.try_into().ok()
}

fn parse_mdhd_timescale(b: RawBox<'_>) -> Option<u32> {
    let (version, _, data) = b.fullbox_payload()?;
    if version == 1 {
        read_u32(data, 16)
    } else {
        read_u32(data, 8)
    }
}

fn parse_stsd(b: RawBox<'_>) -> (String, Option<String>, (u32, u32)) {
    let Some((_, _, data)) = b.fullbox_payload() else {
        return ("unknown".into(), None, (0, 0));
    };
    if data.len() < 8 {
        return ("unknown".into(), None, (0, 0));
    }
    // entry_count then sample entries
    let entries = boxes::iter_boxes(&data[4..], b.file_offset + b.header_len + 8);
    let Some(entry) = entries.first() else {
        return ("unknown".into(), None, (0, 0));
    };
    let codec = fourcc(&entry.typ);
    let mut wh = (0u32, 0u32);
    let mut profile = None;
    // VisualSampleEntry: 78 bytes of fixed fields after the box header, then extra boxes.
    // payload of avc1 starts with 6 reserved + data_ref (2) = 8, then pre_defined...
    if entry.payload.len() >= 24 {
        let w = u16::from_be_bytes([entry.payload[24], entry.payload[25]]) as u32;
        let h = u16::from_be_bytes([entry.payload[26], entry.payload[27]]) as u32;
        if w > 0 && h > 0 {
            wh = (w, h);
        }
    }
    if entry.payload.len() > 78 {
        for child in boxes::iter_boxes(&entry.payload[78..], 0) {
            if child.is_type(b"avcC") && !child.payload.is_empty() {
                profile = Some(avc_profile_name(child.payload[1]));
            }
        }
    }
    (codec, profile, wh)
}

fn avc_profile_name(idc: u8) -> String {
    match idc {
        66 => "Baseline".into(),
        77 => "Main".into(),
        88 => "Extended".into(),
        100 => "High".into(),
        110 => "High 10".into(),
        122 => "High 4:2:2".into(),
        244 => "High 4:4:4".into(),
        other => format!("profile {other}"),
    }
}

fn parse_stts_fps(b: RawBox<'_>) -> Option<u32> {
    let (_, _, data) = b.fullbox_payload()?;
    let count = read_u32(data, 0)? as usize;
    if count == 0 || data.len() < 12 {
        return None;
    }
    // first entry delta
    read_u32(data, 8).filter(|&d| d > 0)
}

fn parse_stbl_samples(stbl: &RawBox<'_>, is_video: bool) -> Vec<Sample> {
    let sizes = stbl
        .child(&b"stsz")
        .map(parse_stsz)
        .or_else(|| stbl.child(&b"stz2").map(parse_stsz))
        .unwrap_or_default();
    if sizes.is_empty() {
        return Vec::new();
    }
    let chunks = stbl
        .child(&b"co64")
        .map(parse_co64)
        .or_else(|| stbl.child(&b"stco").map(parse_stco))
        .unwrap_or_default();
    let stsc = stbl.child(&b"stsc").map(parse_stsc).unwrap_or_default();
    let sync = stbl.child(&b"stss").map(parse_stss);
    let ctts = stbl.child(&b"ctts").map(parse_ctts).unwrap_or_default();

    let per_chunk = samples_per_chunk(&stsc, chunks.len());
    let mut samples = Vec::with_capacity(sizes.len());
    let mut sample_idx = 0usize;
    for (ci, &chunk_off) in chunks.iter().enumerate() {
        let n = *per_chunk.get(ci).unwrap_or(&0) as usize;
        let mut off = chunk_off;
        for _ in 0..n {
            if sample_idx >= sizes.len() {
                break;
            }
            let size = sizes[sample_idx] as u64;
            let number = sample_idx as u32 + 1;
            let is_sync = match &sync {
                Some(set) => set.contains(&number),
                None => !is_video, // audio: treat as sync; video without stss: all sync
            };
            let is_sync = if is_video && sync.is_none() {
                true
            } else if is_video {
                is_sync
            } else {
                false
            };
            let is_b = is_video && !is_sync && ctts.get(sample_idx).copied().unwrap_or(0) != 0;
            samples.push(Sample {
                offset: off,
                size,
                is_sync,
                is_b,
            });
            off = off.saturating_add(size);
            sample_idx += 1;
        }
    }
    samples
}

fn samples_per_chunk(stsc: &[(u32, u32)], chunk_count: usize) -> Vec<u32> {
    let mut out = vec![0u32; chunk_count];
    if stsc.is_empty() || chunk_count == 0 {
        return out;
    }
    for (i, &(first_chunk, spc)) in stsc.iter().enumerate() {
        let start = (first_chunk as usize).saturating_sub(1);
        let end = stsc
            .get(i + 1)
            .map(|(n, _)| (*n as usize).saturating_sub(1))
            .unwrap_or(chunk_count);
        for c in start..end.min(chunk_count) {
            out[c] = spc;
        }
    }
    out
}

fn parse_stsz(b: RawBox<'_>) -> Vec<u32> {
    let Some((_, _, data)) = b.fullbox_payload() else {
        return Vec::new();
    };
    let Some(sample_size) = read_u32(data, 0) else {
        return Vec::new();
    };
    let Some(count) = read_u32(data, 4) else {
        return Vec::new();
    };
    if sample_size != 0 {
        return vec![sample_size; count as usize];
    }
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        match read_u32(data, 8 + i * 4) {
            Some(sz) => out.push(sz),
            None => break,
        }
    }
    out
}

fn parse_stco(b: RawBox<'_>) -> Vec<u64> {
    let Some((_, _, data)) = b.fullbox_payload() else {
        return Vec::new();
    };
    let Some(count) = read_u32(data, 0) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        match read_u32(data, 4 + i * 4) {
            Some(off) => out.push(off as u64),
            None => break,
        }
    }
    out
}

fn parse_co64(b: RawBox<'_>) -> Vec<u64> {
    let Some((_, _, data)) = b.fullbox_payload() else {
        return Vec::new();
    };
    let Some(count) = read_u32(data, 0) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        match read_u64(data, 4 + i * 8) {
            Some(off) => out.push(off),
            None => break,
        }
    }
    out
}

fn parse_stsc(b: RawBox<'_>) -> Vec<(u32, u32)> {
    let Some((_, _, data)) = b.fullbox_payload() else {
        return Vec::new();
    };
    let Some(count) = read_u32(data, 0) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let base = 4 + i * 12;
        match (read_u32(data, base), read_u32(data, base + 4)) {
            (Some(first), Some(spc)) => out.push((first, spc)),
            _ => break,
        }
    }
    out
}

fn parse_stss(b: RawBox<'_>) -> std::collections::HashSet<u32> {
    let mut set = std::collections::HashSet::new();
    let Some((_, _, data)) = b.fullbox_payload() else {
        return set;
    };
    let Some(count) = read_u32(data, 0) else {
        return set;
    };
    for i in 0..count as usize {
        if let Some(n) = read_u32(data, 4 + i * 4) {
            set.insert(n);
        }
    }
    set
}

fn parse_ctts(b: RawBox<'_>) -> Vec<i32> {
    let Some((version, _, data)) = b.fullbox_payload() else {
        return Vec::new();
    };
    let Some(count) = read_u32(data, 0) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..count as usize {
        let base = 4 + i * 8;
        let Some(sc) = read_u32(data, base) else {
            break;
        };
        let offset = if version == 1 {
            read_i32(data, base + 4).unwrap_or(0)
        } else {
            read_u32(data, base + 4).unwrap_or(0) as i32
        };
        for _ in 0..sc {
            out.push(offset);
        }
    }
    out
}

/// Fragmented MP4: map `trun` samples into `mdat`. First sample of each trun is treated
/// as a sync sample when sample flags are absent (same rule as the original CLI).
fn apply_fragments(tracks: &mut [TrackLayout], moofs: &[&TopLevelBox]) {
    for moof in moofs {
        let Some(payload) = moof.payload.as_deref() else {
            continue;
        };
        let base = moof.offset + moof.header_len;
        let moof_box = RawBox {
            file_offset: moof.offset,
            header_len: moof.header_len,
            size: moof.size,
            typ: *b"moof",
            payload,
        };
        for traf in moof_box.descendants(&b"traf") {
            let Some(tfhd) = traf.child(&b"tfhd") else {
                continue;
            };
            let Some((track_id, default_size, default_flags, base_offset)) =
                parse_tfhd(tfhd, moof.offset)
            else {
                continue;
            };
            let Some(track) = tracks.iter_mut().find(|t| track_id_of(t) == Some(track_id)) else {
                continue;
            };
            let Some(trun) = traf.child(&b"trun") else {
                continue;
            };
            if let Some(extra) = parse_trun(
                trun,
                base_offset,
                default_size,
                default_flags,
                track.is_video,
            ) {
                track.samples.extend(extra);
            }
        }
        let _ = base;
    }
}

fn track_id_of(t: &TrackLayout) -> Option<u32> {
    t.info_video
        .as_ref()
        .map(|v| v.track_id)
        .or_else(|| t.info_audio.as_ref().map(|a| a.track_id))
}

fn parse_tfhd(b: RawBox<'_>, moof_offset: u64) -> Option<(u32, u32, u32, u64)> {
    let (_, flags, data) = b.fullbox_payload()?;
    let track_id = read_u32(data, 0)?;
    let mut at = 4usize;
    let mut base_offset = moof_offset;
    if flags & 0x000001 != 0 {
        base_offset = read_u64(data, at)?;
        at += 8;
    }
    if flags & 0x000002 != 0 {
        at += 4; // sample_description_index
    }
    if flags & 0x000008 != 0 {
        at += 4; // default_sample_duration
    }
    let default_size = if flags & 0x000010 != 0 {
        let v = read_u32(data, at)?;
        at += 4;
        v
    } else {
        0
    };
    let default_flags = if flags & 0x000020 != 0 {
        read_u32(data, at)?
    } else {
        0
    };
    Some((track_id, default_size, default_flags, base_offset))
}

fn parse_trun(
    b: RawBox<'_>,
    base_offset: u64,
    default_size: u32,
    default_flags: u32,
    is_video: bool,
) -> Option<Vec<Sample>> {
    let (_, flags, data) = b.fullbox_payload()?;
    let sample_count = read_u32(data, 0)? as usize;
    let mut at = 4usize;
    let mut offset = base_offset;
    if flags & 0x000001 != 0 {
        let data_offset = read_i32(data, at)? as i64;
        offset = (base_offset as i64 + data_offset) as u64;
        at += 4;
    }
    if flags & 0x000004 != 0 {
        at += 4; // first_sample_flags
    }
    let mut samples = Vec::with_capacity(sample_count);
    for i in 0..sample_count {
        if flags & 0x000100 != 0 {
            at += 4; // duration
        }
        let size = if flags & 0x000200 != 0 {
            let v = read_u32(data, at)?;
            at += 4;
            v
        } else {
            default_size
        };
        let sample_flags = if flags & 0x000400 != 0 {
            let v = read_u32(data, at)?;
            at += 4;
            v
        } else {
            default_flags
        };
        if flags & 0x000800 != 0 {
            at += 4; // composition offset
        }
        let non_sync = sample_flags & 0x0001_0000 != 0;
        let is_sync = if flags & 0x000400 != 0 || default_flags != 0 {
            !non_sync
        } else {
            i == 0 && is_video
        };
        samples.push(Sample {
            offset,
            size: size as u64,
            is_sync: is_sync && is_video,
            is_b: false,
        });
        offset = offset.saturating_add(size as u64);
    }
    Some(samples)
}
