// muguet - audio.rs
// Constantin Clerc (c22dev) - 0.1
// The game ring buffer holds 48 kHz PCM. Many Bluetooth A2DP outputs (e.g. Sony
// WH-1000XM series) only run at 44.1 kHz; requiring 48 kHz unconditionally makes
// cpal fail with "Sample rate 48000 Hz is not supported" and the game ends up
// with no sound at all. Try 48 kHz first, then fall back to the device's native
// rate and linearly resample 48 kHz -> device rate.


use crate::RT;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};

static UNDERRUN_FRAMES: AtomicU64 = AtomicU64::new(0);
static CALLBACKS: AtomicU64 = AtomicU64::new(0);

static STARTED: AtomicBool = AtomicBool::new(false);

pub fn start(ring: u64, capacity: u32) {
    if STARTED.swap(true, Ordering::SeqCst) || std::env::var("MUGUET_MUTE").is_ok() {
        return;
    }

    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || match play(ring, capacity) {
            Ok(stream) => {
                let _keep = stream;
                loop { std::thread::park(); }
            }
            Err(e) => eprintln!("[audio] no output: {e:#}"),
        })
        .expect("spawn audio thread");
}

#[inline]
fn hdr_at(base: usize, ring: u64, i: usize) -> &'static AtomicI32 {
    unsafe { &*((base + ring as usize + 4 * i) as *const AtomicI32) }
}

#[inline]
fn data_at(base: usize, ring: u64) -> usize {
    base + ring as usize + 64
}

fn play(ring: u64, capacity: u32) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or_else(|| anyhow::anyhow!("no default output device"))?;
    let (base, _) = crate::mem_base_len(&RT.get().unwrap().mem);
    let base = base as usize;
    let mask = capacity.wrapping_sub(1);

    let mut rates = vec![48_000u32];
    if let Ok(c) = device.default_output_config() {
        let native = c.sample_rate();
        if rates[0] != native { rates.push(native); }
    }

    let mut last_err: Option<anyhow::Error> = None;
    for rate in rates {
        let ratio = 48_000.0f64 / rate as f64; // ring frames consumed per output frame
        let mut abs = 0.0f64;                  // absolute fractional read pos in ring counter space
        let mut init = false;
        let cfg = cpal::StreamConfig { channels: 2, sample_rate: rate, buffer_size: cpal::BufferSize::Default };
        match device.build_output_stream(cfg, move |out: &mut [f32], _| {
            let hdr = |i: usize| hdr_at(base, ring, i);
            let frames = out.len() / 2;
            let read = hdr(1).load(Ordering::SeqCst) as u32;
            let write = hdr(0).load(Ordering::SeqCst) as u32;
            let avail = write.wrapping_sub(read) as usize;

            if ratio == 1.0 {
                let take = avail.min(frames);
                for i in 0..take {
                    let at = ((read.wrapping_add(i as u32) & mask) * 2) as usize;
                    unsafe {
                        let p = (data_at(base, ring) as *const f32).add(at);
                        out[2 * i] = *p;
                        out[2 * i + 1] = *p.add(1);
                    }
                }
                for s in &mut out[take * 2..] { *s = 0.0; }
                UNDERRUN_FRAMES.fetch_add((frames - take) as u64, Ordering::Relaxed);
                CALLBACKS.fetch_add(1, Ordering::Relaxed);
                hdr(1).store(read.wrapping_add(take as u32) as i32, Ordering::SeqCst);
                hdr(3).fetch_add(1, Ordering::Relaxed);
                return;
            }

            // resampling path: linear interpolation over the 48 kHz ring
            if !init || read.wrapping_sub(abs as u32) as usize > avail.max(1) { abs = read as f64; init = true; } // (re)sync
            let mut produced = 0usize;
            for i in 0..frames {
                let s = abs + i as f64 * ratio;
                let i0 = s as u32; // floor in ring counter space (wraps with the counters)
                let rel = i0.wrapping_sub(read) as usize;
                if rel + 1 >= avail { break; } // need i0 and i0+1
                let t = (s - i0 as f64) as f32;
                unsafe {
                    let pa = (data_at(base, ring) as *const f32).add(((i0 & mask) * 2) as usize);
                    let pb = (data_at(base, ring) as *const f32).add((((i0 + 1) & mask) * 2) as usize);
                    out[2 * i] = *pa + (*pb - *pa) * t;
                    out[2 * i + 1] = *pa.add(1) + (*pb.add(1) - *pa.add(1)) * t;
                }
                produced = i + 1;
            }
            for s in &mut out[produced * 2..] { *s = 0.0; }
            UNDERRUN_FRAMES.fetch_add(((frames - produced) as f64 * ratio) as u64, Ordering::Relaxed);
            CALLBACKS.fetch_add(1, Ordering::Relaxed);
            abs += produced as f64 * ratio;
            if abs >= 4_294_967_296.0 { abs -= 4_294_967_296.0; } // keep inside u32 counter space
            hdr(1).store(abs as u32 as i32, Ordering::SeqCst);
            hdr(3).fetch_add(1, Ordering::Relaxed);
        }, |e| eprintln!("[audio] stream error: {e}"), None) {
            Ok(stream) => {
                stream.play()?;
                eprintln!("[audio] CoreAudio output started (ring 0x{ring:x}, {capacity} frames, {rate} Hz{})",
                    if rate == 48_000 { "" } else { ", resampling from 48 kHz" });
                std::thread::spawn(move || {
                    let mut last_w = hdr_at(base, ring, 0).load(Ordering::SeqCst) as u32;
                    let mut last_u = 0;
                    let mut last_c = 0;
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        let w = hdr_at(base, ring, 0).load(Ordering::SeqCst) as u32;
                        let (u, c) = (UNDERRUN_FRAMES.load(Ordering::Relaxed), CALLBACKS.load(Ordering::Relaxed));
                        eprintln!("[audio] mixer produced {:.0} frames/s (needs 48000), underrun {:.0} frames/s, {} callbacks/s, ring fill {}",
                            w.wrapping_sub(last_w) as f64 / 5.0, (u - last_u) as f64 / 5.0, (c - last_c) / 5, w.wrapping_sub(hdr_at(base, ring, 1).load(Ordering::SeqCst) as u32));
                        last_w = w; last_u = u; last_c = c;
                    }
                });
                return Ok(stream);
            }
            Err(e) => {
                eprintln!("[audio] output device rejected {rate} Hz: {e}");
                last_err = Some(e.into());
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no supported sample rate")))
}
