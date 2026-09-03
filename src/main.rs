use anyhow::Result;
use bitplay::{self, SpanKind};
use std::env;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: cargo run -- <video.mp4>");
        return Ok(());
    }
    let path = &args[1];
    let analysis = bitplay::analyze(path.as_ref())?;

    println!("\n==============================");
    println!("          MP4 METADATA");
    println!("==============================\n");
    println!("File:");
    println!("{path}");
    println!("\nSize:");
    println!("{}", bitplay::human_bytes(analysis.file_size));
    println!("\nDuration:");
    match analysis.duration_secs {
        Some(d) => println!("{d:.3} seconds"),
        None => println!("Unknown"),
    }
    println!("\nTimescale:");
    match analysis.timescale {
        Some(t) => println!("{t}"),
        None => println!("Unknown"),
    }
    if !analysis.moov_present {
        println!("\nWarning: no moov atom — file looks truncated. Use iframe extraction.");
    }

    println!("\n==============================");
    println!("        VIDEO INFORMATION");
    println!("==============================\n");
    if let Some(v) = &analysis.video {
        println!("Track ID       : {}", v.track_id);
        println!("Codec          : {}", v.codec);
        println!("Resolution     : {} x {}", v.width, v.height);
        match v.fps {
            Some(fps) => println!("FPS            : {fps:.3}"),
            None => println!("FPS            : Unknown"),
        }
        println!(
            "Profile        : {}",
            v.profile.as_deref().unwrap_or("Unknown")
        );
        println!("Samples        : {}", v.sample_count);
        println!("I-frames       : {}", v.iframe_count);
        println!("GOPs           : {}", v.gop_count);
    } else {
        println!("No video track indexed.");
    }

    println!("\n==============================");
    println!("        LAYOUT SUMMARY");
    println!("==============================\n");
    let mut i_count = 0u32;
    let mut p_count = 0u32;
    let mut b_count = 0u32;
    let mut audio_bytes = 0u64;
    let mut video_bytes = 0u64;
    for s in &analysis.spans {
        match s.kind {
            SpanKind::VideoI { .. } => {
                i_count += 1;
                video_bytes += s.size;
            }
            SpanKind::VideoP { .. } => {
                p_count += 1;
                video_bytes += s.size;
            }
            SpanKind::VideoB { .. } => {
                b_count += 1;
                video_bytes += s.size;
            }
            SpanKind::Audio => audio_bytes += s.size,
            _ => {}
        }
    }
    println!("I-frames : {i_count}");
    println!("P-frames : {p_count}");
    println!("B-frames : {b_count}");
    println!("Video    : {}", bitplay::human_bytes(video_bytes));
    println!("Audio    : {}", bitplay::human_bytes(audio_bytes));

    if let Some(a) = &analysis.audio {
        println!("\n==============================");
        println!("        AUDIO INFORMATION");
        println!("==============================\n");
        println!("Track ID       : {}", a.track_id);
        println!("Codec          : {}", a.codec);
        println!("Samples        : {}", a.sample_count);
    }

    println!("\n==============================");
    println!("          COMPLETE");
    println!("==============================");
    Ok(())
}
