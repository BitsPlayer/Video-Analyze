use anyhow::Result;
use mp4::Mp4Reader;
use serde_json::Value;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::process::Command;


fn main() -> Result<()> {

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage: cargo run -- <video.mp4>");
        return Ok(());
    }


    let path = &args[1];


    // Open MP4

    let file = File::open(path)?;
    let size = file.metadata()?.len();

    let reader = BufReader::new(file);

    let mp4 = Mp4Reader::read_header(reader, size)?;



    // ==========================
    // Get FFPROBE DATA
    // ==========================


    let mut duration = String::from("Unknown");
    let mut fps = String::from("Unknown");
    let mut profile = String::from("Unknown");
    let mut encoder = String::from("Unknown");


    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
            path,
        ])
        .output();


    if let Ok(result) = output {

        let json: Value =
            serde_json::from_slice(&result.stdout)?;



        if let Some(format) = json["format"].as_object() {

            if let Some(dur) = format.get("duration") {
                duration = dur
                    .as_str()
                    .unwrap_or("Unknown")
                    .to_string();
            }

        }



        if let Some(streams) = json["streams"].as_array() {

            for stream in streams {


                if stream["codec_type"] == "video" {


                    if let Some(value)=stream["r_frame_rate"].as_str() {

                        fps=value.to_string();

                    }


                    if let Some(value)=stream["profile"].as_str(){

                        profile=value.to_string();

                    }


                    if let Some(value)=stream["encoder"].as_str(){

                        encoder=value.to_string();

                    }


                }


            }

        }


    }



    // ==========================
    // Collect track information
    // ==========================


    let mut video_track = None;
    let mut audio_track = None;


    for track in mp4.tracks().values() {


        if let Ok(track_type)=track.track_type(){


            if format!("{:?}",track_type).contains("Video") {

                video_track=Some(track);

            }


            else if format!("{:?}",track_type).contains("Audio") {

                audio_track=Some(track);

            }

        }

    }



    // ==========================
    // PRINT METADATA
    // ==========================


    println!("\n==============================");
    println!("          MP4 METADATA");
    println!("==============================\n");


    println!("File:");
    println!("{}",path);


    println!("\nDuration:");
    println!("{} seconds",duration);


    println!("\nTimescale:");
    println!("{}",mp4.timescale());



    println!("\n==============================");
    println!("        VIDEO INFORMATION");
    println!("==============================\n");



    if let Some(track)=video_track {


        println!("Track ID       : {}",track.track_id());

        println!("Codec          : {:?}",track.media_type()?);

        println!(
            "Resolution     : {} x {}",
            track.width(),
            track.height()
        );


        println!("FPS            : {}",fps);

        println!("Profile        : {}",profile);

        println!("Encoder        : {}",encoder);


        println!("Samples        : {}",track.sample_count());

        println!("Fragments      : {}",track.trafs.len());



        // ==========================
        // I FRAME INFORMATION
        // ==========================


        println!("\n==============================");
        println!("        I FRAME INFORMATION");
        println!("==============================\n");


        let mut iframe_count=0;



        for traf in &track.trafs {


            let tfhd=&traf.tfhd;


            if let Some(base_offset)=tfhd.base_data_offset {


                if let Some(trun)=&traf.trun {


                    let mut offset =
                        base_offset +
                        trun.data_offset.unwrap_or(0) as u64;



                    for (i,size) in trun.sample_sizes.iter().enumerate(){


                        // First sample of fragment = key frame

                        if i==0 {


                            println!(
                                "I-frame {:3} | Offset {:10} | Size {} bytes",
                                iframe_count,
                                offset,
                                size
                            );


                            iframe_count+=1;

                        }


                        offset+=*size as u64;


                    }

                }

            }

        }


        println!(
            "\nTotal I-frames : {}",
            iframe_count
        );


    }



    // ==========================
    // AUDIO INFORMATION
    // ==========================


    println!("\n==============================");
    println!("        AUDIO INFORMATION");
    println!("==============================\n");


    if let Some(track)=audio_track {


        println!("Track ID       : {}",track.track_id());

        println!("Codec          : {:?}",track.media_type()?);

        println!("Samples        : {}",track.sample_count());


    }


    println!("\n==============================");
    println!("          COMPLETE");
    println!("==============================");


    Ok(())
}