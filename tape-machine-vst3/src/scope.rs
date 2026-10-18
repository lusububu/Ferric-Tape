// Shared state for the GUI's live meters: the hysteresis scope (record signal vs recovered
// signal, same pair TapeChannel already tracks as sx/sy for exactly this purpose) and the L/R
// peak meters. Written from the audio thread in TapeMachine::process_rt, read from the UI thread
// in editor.rs. No heap allocation happens on the audio-thread side: the scope trace lives in a
// fixed-size array behind a Mutex that's only ever try_lock'd (a GUI frame that loses the race
// just reuses the previous trace), and the peak meters are plain atomics.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

pub const SCOPE_CAPACITY: usize = 512;

pub struct ScopeState {
    pts: Mutex<[[f32; 2]; SCOPE_CAPACITY]>,
    len: AtomicUsize,
    peak_l_bits: AtomicU32,
    peak_r_bits: AtomicU32,
}

impl ScopeState {
    pub fn new() -> Self {
        ScopeState {
            pts: Mutex::new([[0.0; 2]; SCOPE_CAPACITY]),
            len: AtomicUsize::new(0),
            peak_l_bits: AtomicU32::new(0),
            peak_r_bits: AtomicU32::new(0),
        }
    }

    /// Audio thread only. `block_peak_l`/`block_peak_r` are this block's peak absolute sample;
    /// `decay` (0..1) is how much of the previously held peak survives this block, giving the
    /// meter a VU-style release instead of jumping straight to silence between loud blocks.
    pub fn update_peaks(&self, block_peak_l: f32, block_peak_r: f32, decay: f32) {
        let prev_l = f32::from_bits(self.peak_l_bits.load(Ordering::Relaxed));
        let prev_r = f32::from_bits(self.peak_r_bits.load(Ordering::Relaxed));
        self.peak_l_bits.store(block_peak_l.max(prev_l * decay).to_bits(), Ordering::Relaxed);
        self.peak_r_bits.store(block_peak_r.max(prev_r * decay).to_bits(), Ordering::Relaxed);
    }

    pub fn peak(&self, channel: usize) -> f32 {
        f32::from_bits(if channel == 0 { self.peak_l_bits.load(Ordering::Relaxed) } else { self.peak_r_bits.load(Ordering::Relaxed) })
    }

    /// Audio thread only, called once per `process()` block with a closure that yields each
    /// sample's (sx, sy) scope point in order; evenly subsampled down to SCOPE_CAPACITY points if
    /// the block has more samples than that. Never blocks: if the UI thread currently holds the
    /// lock, this call just skips publishing a new trace for this block.
    pub fn publish_trace(&self, num_samples: usize, mut point_at: impl FnMut(usize) -> [f32; 2]) {
        if num_samples == 0 {
            return;
        }
        if let Ok(mut buf) = self.pts.try_lock() {
            let out_n = num_samples.min(SCOPE_CAPACITY);
            for i in 0..out_n {
                let src = i * num_samples / out_n;
                buf[i] = point_at(src);
            }
            self.len.store(out_n, Ordering::Relaxed);
        }
    }

    /// UI thread: a snapshot copy of the current trace (allocates, but this runs off the audio
    /// thread so that's fine).
    pub fn snapshot_trace(&self) -> Vec<[f32; 2]> {
        let len = self.len.load(Ordering::Relaxed);
        match self.pts.lock() {
            Ok(buf) => buf[..len].to_vec(),
            Err(_) => Vec::new(),
        }
    }
}
