//! Copy child audio while OBS owns the child lock, then deliver it from a
//! separate thread. Never call the parent source from a child audio callback:
//! OBS may concurrently hold the parent lock while querying the child size.
use crate::obs::{self, AudioData, AudioInfo, Obs, SourceAudio};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const QUEUE_PACKETS: usize = 32;
const MAX_FRAMES: u32 = 8192;

struct Packet {
    samples: [Vec<f32>; 8],
    frames: u32,
    speakers: i32,
    sample_rate: u32,
    timestamp: u64,
}

pub struct AudioBridge {
    sender: SyncSender<Packet>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl AudioBridge {
    pub fn new(parent: *mut Obs) -> Result<Self, std::io::Error> {
        let (sender, receiver) = mpsc::sync_channel::<Packet>(QUEUE_PACKETS);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        // OBS keeps the parent alive until its destroy callback joins this worker.
        let parent_address = parent as usize;
        let worker = thread::Builder::new()
            .name("rtsps-secure-audio".into())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(packet) if !worker_stop.load(Ordering::Acquire) => {
                            let mut data = [ptr::null(); 8];
                            for (i, samples) in packet.samples.iter().enumerate() {
                                if !samples.is_empty() {
                                    data[i] = samples.as_ptr().cast::<u8>();
                                }
                            }
                            let output = SourceAudio {
                                data,
                                frames: packet.frames,
                                speakers: packet.speakers,
                                format: 8, // AUDIO_FORMAT_FLOAT_PLANAR
                                samples_per_sec: packet.sample_rate,
                                timestamp: packet.timestamp,
                            };
                            // No child-source lock or queue lock is held here.
                            unsafe {
                                obs::obs_source_output_audio(parent_address as *mut Obs, &output)
                            };
                        }
                        Ok(_) => break,
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })?;
        Ok(Self {
            sender,
            stop,
            worker: Some(worker),
        })
    }

    /// Called synchronously by OBS; never block waiting for the parent or queue.
    pub unsafe fn enqueue(&self, input: &AudioData, info: &AudioInfo) {
        if self.stop.load(Ordering::Acquire) || input.frames == 0 || input.frames > MAX_FRAMES {
            return;
        }
        let channels = match info.speakers {
            1..=6 => info.speakers as usize,
            8 => 8,
            _ => return,
        };
        if info.samples_per_sec == 0 || input.data[..channels].iter().any(|p| p.is_null()) {
            return;
        }
        let mut samples: [Vec<f32>; 8] = std::array::from_fn(|_| Vec::new());
        for (index, out) in samples.iter_mut().enumerate().take(channels) {
            // OBS capture callbacks provide planar float audio with valid data
            // for the duration of this callback. Copy before it returns.
            *out = unsafe {
                std::slice::from_raw_parts(input.data[index].cast::<f32>(), input.frames as usize)
            }
            .to_vec();
        }
        let packet = Packet {
            samples,
            frames: input.frames,
            speakers: info.speakers,
            sample_rate: info.samples_per_sec,
            timestamp: input.timestamp,
        };
        // Bounded queue: if OBS is too busy, drop this packet instead of
        // blocking the media thread and causing another lock inversion.
        let _ = self.sender.try_send(packet);
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for AudioBridge {
    fn drop(&mut self) {
        self.stop();
    }
}
