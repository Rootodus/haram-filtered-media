use std::fs::File;
use std::mem::MaybeUninit;
use std::ptr::NonNull;
use std::sync::Once;

use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::default::get_probe;

use rav1d::include::dav1d::data::Dav1dData;
use rav1d::include::dav1d::dav1d::{Dav1dContext, Dav1dSettings};
use rav1d::include::dav1d::picture::Dav1dPicture;
use rav1d::src::lib::{
    dav1d_close, dav1d_data_create, dav1d_data_unref, dav1d_default_settings, dav1d_flush,
    dav1d_get_picture, dav1d_open, dav1d_picture_unref, dav1d_send_data,
};

use image::RgbImage;

static SAVE_FRAME: Once = Once::new();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "assets/video.mp4";
    println!("[MAIN] Opening file: {}", file_path);
    let source = File::open(file_path)?;
    let mss = MediaSourceStream::new(Box::new(source), Default::default());

    let hint = Hint::new();
    let format_opts = FormatOptions::default();
    let meta_opts = MetadataOptions::default();

    println!("[MAIN] Probing file...");
    let mut format_reader = get_probe()
        .probe(&hint, mss, format_opts, meta_opts)
        .expect("Failed to probe file");
    println!("[MAIN] Probe successful");

    for (i, track) in format_reader.tracks().iter().enumerate() {
        println!(
            "[MAIN] Track {}: id={}, codec_params={:?}",
            i, track.id, track.codec_params
        );
    }

    let video_track = format_reader
        .tracks()
        .iter()
        .find(|track| {
            track
                .codec_params
                .as_ref()
                .map_or(false, |cp| cp.is_video())
        })
        .cloned()
        .expect("No video track");

    println!("[MAIN] Using video track id: {}", video_track.id);

    // --- Initialize rav1d context ---
    println!("[MAIN] Initializing rav1d...");
    let mut settings = MaybeUninit::<Dav1dSettings>::uninit();
    unsafe {
        dav1d_default_settings(NonNull::new(settings.as_mut_ptr()).unwrap());
    }
    let settings = unsafe { settings.assume_init() };

    let mut ctx_ptr: Option<Dav1dContext> = None;
    let res = unsafe {
        dav1d_open(
            Some(NonNull::from(&mut ctx_ptr)),
            Some(NonNull::from(&settings)),
        )
    };
    if res.0 != 0 {
        panic!("dav1d_open failed: {}", res.0);
    }
    let ctx = ctx_ptr.unwrap();
    println!("[MAIN] rav1d context created");

    let mut obu_buffer = Vec::new();
    let mut packet_count = 0;

    while let Ok(Some(packet)) = format_reader.next_packet() {
        packet_count += 1;
        if packet_count % 10 == 0 {
            println!(
                "[MAIN] Packet #{} received, track={}, size={}",
                packet_count,
                packet.track_id,
                packet.data.len()
            );
        }

        if packet.track_id != video_track.id {
            continue;
        }

        println!(
            "[MAIN] Processing video packet #{} (size={})",
            packet_count,
            packet.data.len()
        );
        if packet.data.len() >= 16 {
            println!("[MAIN] Packet first 16 bytes: {:02x?}", &packet.data[..16]);
        }

        // Append raw packet data to the OBU buffer
        obu_buffer.extend_from_slice(&packet.data);
        println!("[MAIN] OBU buffer size: {}", obu_buffer.len());

        if !obu_buffer.is_empty() {
            // --- Use dav1d_data_create to allocate internal buffer ---
            let mut dav1d_data = Dav1dData::default();
            // `dav1d_data_create` returns a pointer to the allocated buffer.
            let buf_ptr = unsafe {
                dav1d_data_create(Some(NonNull::from(&mut dav1d_data)), obu_buffer.len())
            };
            if buf_ptr.is_null() {
                eprintln!("[MAIN] dav1d_data_create failed (null pointer)");
                obu_buffer.clear();
                continue;
            }
            // Copy our OBU data into the allocated buffer.
            unsafe {
                std::ptr::copy_nonoverlapping(obu_buffer.as_ptr(), buf_ptr, obu_buffer.len());
            }
            println!("[MAIN] OBU data copied, sending to decoder");

            let res = unsafe { dav1d_send_data(Some(ctx), Some(NonNull::from(&mut dav1d_data))) };
            if res.0 != 0 {
                // On error, unref the data to free the internal buffer.
                unsafe { dav1d_data_unref(Some(NonNull::from(&mut dav1d_data))) }
                obu_buffer.clear();
                if res.0 == -11 {
                    println!("[MAIN] send_data EAGAIN, need more data");
                    continue;
                } else {
                    eprintln!("[MAIN] send_data error: {}", res.0);
                    continue;
                }
            }
            obu_buffer.clear();
            println!("[MAIN] send_data success");

            // Try to get picture
            loop {
                let mut picture = MaybeUninit::<Dav1dPicture>::uninit();
                let res = unsafe {
                    dav1d_get_picture(Some(ctx), Some(NonNull::new(picture.as_mut_ptr()).unwrap()))
                };
                if res.0 == 0 {
                    let mut picture = unsafe { picture.assume_init() };
                    let w = picture.p.w as usize;
                    let h = picture.p.h as usize;
                    println!("[MAIN] Decoded frame: {}x{}", w, h);
                    let y_ptr = picture.data[0].unwrap().as_ptr() as *const u8;
                    let u_ptr = picture.data[1].unwrap().as_ptr() as *const u8;
                    let v_ptr = picture.data[2].unwrap().as_ptr() as *const u8;
                    let y_stride = picture.stride[0] as usize;
                    let uv_stride = picture.stride[1] as usize;

                    let mut rgb = Vec::with_capacity(w * h * 3);
                    for row in 0..h {
                        for col in 0..w {
                            let y_idx = row * y_stride + col;
                            let uv_idx = (row / 2) * uv_stride + (col / 2);
                            unsafe {
                                let y_val = *y_ptr.add(y_idx) as f32;
                                let u_val = *u_ptr.add(uv_idx) as f32 - 128.0;
                                let v_val = *v_ptr.add(uv_idx) as f32 - 128.0;
                                let r = (y_val + 1.5748 * v_val).clamp(0.0, 255.0) as u8;
                                let g = (y_val - 0.1873 * u_val - 0.4681 * v_val).clamp(0.0, 255.0)
                                    as u8;
                                let b = (y_val + 1.8556 * u_val).clamp(0.0, 255.0) as u8;
                                rgb.push(r);
                                rgb.push(g);
                                rgb.push(b);
                            }
                        }
                    }

                    SAVE_FRAME.call_once(|| {
                        if let Some(img) = RgbImage::from_raw(w as u32, h as u32, rgb) {
                            if let Err(e) = img.save("symphonia_rav1d_frame.png") {
                                eprintln!("[MAIN] Failed to save PNG: {}", e);
                            } else {
                                println!("[MAIN] Saved symphonia_rav1d_frame.png");
                            }
                        } else {
                            eprintln!("[MAIN] Failed to create RgbImage");
                        }
                    });

                    unsafe { dav1d_picture_unref(Some(NonNull::from(&mut picture))) }
                    if SAVE_FRAME.is_completed() {
                        break;
                    }
                } else if res.0 == -11 {
                    println!("[MAIN] get_picture EAGAIN, no more frames yet");
                    break;
                } else {
                    eprintln!("[MAIN] get_picture error: {}", res.0);
                    break;
                }
            }
        }
        if SAVE_FRAME.is_completed() {
            break;
        }
    }

    // Flush
    println!("[MAIN] Flushing decoder");
    unsafe { dav1d_flush(ctx) }
    // Close
    let mut ctx_ptr2 = Some(ctx);
    unsafe { dav1d_close(Some(NonNull::from(&mut ctx_ptr2))) }
    println!("[MAIN] Done");

    Ok(())
}
