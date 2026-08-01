use anyhow::Result;
use mp4::Mp4Reader;
use std::env;
use std::fs::File;
use std::io::BufReader;


fn calculate_fps(track: &mp4::Mp4Track) -> f64 {

    let mut total_duration: u64 = 0;


    for traf in &track.trafs {

        if let Some(trun) = &traf.trun {


            // If individual sample durations exist
            if !trun.sample_durations.is_empty() {

                for duration in &trun.sample_durations {

                    total_duration += *duration as u64;

                }

            } 
            // Otherwise use default sample duration
            else if let Some(default_duration) =
                traf.tfhd.default_sample_duration {

                total_duration +=
                    default_duration as u64 *
                    trun.sample_count as u64;

            }
        }
    }


    if total_duration == 0 {
        return 0.0;
    }


    // MP4 timescale of your file
    let timescale = 1000.0;


    let seconds = total_duration as f64 / timescale;


    track.sample_count() as f64 / seconds
}



fn main() -> Result<()> {


    let args: Vec<String> = env::args().collect();


    if args.len() < 2 {

        println!("Usage: cargo run -- <video.mp4>");

        return Ok(());

    }


    let path = &args[1];


    let file = File::open(path)?;

    let size = file.metadata()?.len();


    let reader = BufReader::new(file);


    let mp4 = Mp4Reader::read_header(reader, size)?;



    println!("====== Video Metadata ======");


    println!("Duration: {:?}", mp4.duration());


    println!("Timescale: {}", mp4.timescale());



    for track in mp4.tracks().values() {


        println!("\nTrack ID: {}", track.track_id());


        println!("Track Type: {:?}", track.track_type());


        println!("Samples: {}", track.sample_count());


        println!("Codec: {:?}", track.media_type()?);



        if let Ok(track_type) = track.track_type() {


            if format!("{:?}", track_type).contains("Video") {



                println!(
                    "Resolution: {} x {}",
                    track.width(),
                    track.height()
                );



                println!(
                    "FPS: {:.2}",
                    calculate_fps(track)
                );



                println!(
                    "Fragments: {}",
                    track.trafs.len()
                );



                println!("\n===== I Frames =====");



                let mut iframe_count = 0;



                for traf in &track.trafs {


                    let tfhd = &traf.tfhd;



                    if let Some(base_offset) = tfhd.base_data_offset {



                        if let Some(trun) = &traf.trun {



                            let mut offset =
                                base_offset +
                                trun.data_offset.unwrap_or(0) as u64;



                            for (i, size) in
                                trun.sample_sizes.iter().enumerate()
                            {



                                // Temporary assumption:
                                // first sample of every fragment is I-frame

                                if i == 0 {


                                    println!(
                                        "I-frame {} -> Offset: {} Size: {} bytes",
                                        iframe_count,
                                        offset,
                                        size
                                    );


                                    iframe_count += 1;

                                }



                                offset += *size as u64;


                            }

                        }

                    }

                }



                println!(
                    "Total I-frames: {}",
                    iframe_count
                );

            }

        }

    }



    Ok(())

}