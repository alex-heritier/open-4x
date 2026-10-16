//! Generates the base pack's synthesized audio.  All pictures come from the Python pipeline in
//! `art/` (see `art/README.md`); this tool deliberately writes no PNGs.
//!
//! Every sound is computed from scratch (sines, filtered noise and envelopes) with a fixed noise
//! seed, so running the tool twice writes identical files. The combat cues are the eight names
//! of `docs/combat-animation.md` §7.
use std::{f32::consts::PI, path::PathBuf};

const RATE: u32 = 22_050;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/packs/base")
        });
    std::fs::create_dir_all(root.join("audio"))?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::create(root.join("audio/bell.wav"), spec)?;
    for i in 0..11025 {
        let t = i as f32 / 22050.0;
        let value = ((t * PI * 2.0 * 660.0).sin() * 0.55 + (t * PI * 2.0 * 990.0).sin() * 0.2)
            * (-t * 8.0).exp();
        wav.write_sample((value * 16000.0) as i16)?;
    }
    wav.finalize()?;
    println!("Generated bell.wav in {}", root.display());
    for (name, samples) in combat_sounds() {
        let mut wav = hound::WavWriter::create(root.join(format!("audio/{name}.wav")), spec)?;
        for sample in samples {
            wav.write_sample((sample * f32::from(i16::MAX)) as i16)?;
        }
        wav.finalize()?;
        println!("Generated {name}.wav in {}", root.display());
    }
    Ok(())
}

/// A repeatable stream of white noise in `-1..1` (xorshift).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
    /// A number in `from..to`.
    fn between(&mut self, from: f32, to: f32) -> f32 {
        from + (self.next() * 0.5 + 0.5) * (to - from)
    }
}

/// Samples for `seconds`, one call of `voice(t)` per sample. A voice keeps its own state.
fn render(seconds: f32, mut voice: impl FnMut(f32) -> f32) -> Vec<f32> {
    (0..(seconds * RATE as f32) as usize)
        .map(|i| voice(i as f32 / RATE as f32))
        .collect()
}

/// Adds `layer` into `track` from `at` seconds, scaled by `gain`.
fn mix(track: &mut [f32], at: f32, layer: &[f32], gain: f32) {
    let start = (at * RATE as f32) as usize;
    for (slot, sample) in track.iter_mut().skip(start).zip(layer) {
        *slot += sample * gain;
    }
}

/// A one-pole low-pass filter.
struct LowPass {
    state: f32,
    k: f32,
}

impl LowPass {
    fn new(cutoff: f32) -> Self {
        Self {
            state: 0.0,
            k: 1.0 - (-2.0 * PI * cutoff / RATE as f32).exp(),
        }
    }
    fn filter(&mut self, x: f32) -> f32 {
        self.state += self.k * (x - self.state);
        self.state
    }
}

/// A sine whose pitch glides from `from` Hz toward `to` Hz at `rate` per second.
fn glide(from: f32, to: f32, rate: f32) -> impl FnMut(f32) -> f32 {
    let mut phase = 0.0f32;
    move |t| {
        let hz = to + (from - to) * (-t * rate).exp();
        phase += 2.0 * PI * hz / RATE as f32;
        phase.sin()
    }
}

/// A crack of noise with the lows taken out.
fn crack(noise: &mut Noise, seconds: f32, decay: f32) -> Vec<f32> {
    let mut low = LowPass::new(1800.0);
    render(seconds, |t| {
        let n = noise.next();
        (n - low.filter(n)) * (-t * decay).exp()
    })
}

/// Noise with the highs taken out, fading as `exp(-decay t)`.
fn rumble(noise: &mut Noise, seconds: f32, cutoff: f32, decay: f32) -> Vec<f32> {
    let mut low = LowPass::new(cutoff);
    render(seconds, |t| low.filter(noise.next()) * (-t * decay).exp())
}

/// A low thump that drops in pitch.
fn thump(seconds: f32, from: f32, to: f32, decay: f32) -> Vec<f32> {
    let mut tone = glide(from, to, 30.0);
    render(seconds, |t| tone(t) * (-t * decay).exp())
}

fn rifle(noise: &mut Noise) -> Vec<f32> {
    let mut shot = crack(noise, 0.16, 38.0);
    for (s, b) in shot.iter_mut().zip(thump(0.16, 140.0, 70.0, 28.0)) {
        *s += b * 0.35;
    }
    shot
}

fn cannon(noise: &mut Noise, seconds: f32, pitch: f32) -> Vec<f32> {
    let mut track = vec![0.0; (seconds * RATE as f32) as usize];
    mix(&mut track, 0.0, &crack(noise, 0.2, 30.0), 0.8);
    mix(
        &mut track,
        0.0,
        &thump(seconds, 120.0 * pitch, 36.0 * pitch, 7.0),
        0.9,
    );
    mix(&mut track, 0.0, &rumble(noise, seconds, 420.0, 5.0), 1.6);
    track
}

fn volley(noise: &mut Noise) -> Vec<f32> {
    let mut track = vec![0.0; (0.55 * RATE as f32) as usize];
    for k in 0..6 {
        let at = noise.between(0.0, 0.1) + k as f32 * 0.012;
        let gain = noise.between(0.5, 1.0);
        let shot = rifle(noise);
        mix(&mut track, at, &shot, gain);
    }
    mix(&mut track, 0.0, &rumble(noise, 0.55, 700.0, 9.0), 0.5);
    track
}

fn charge(noise: &mut Noise) -> Vec<f32> {
    let mut track = vec![0.0; (0.95 * RATE as f32) as usize];
    // Three strides of three hoofbeats, the beats coming closer and louder.
    for (k, at) in [0.0, 0.09, 0.17, 0.31, 0.39, 0.47, 0.60, 0.68, 0.75]
        .into_iter()
        .enumerate()
    {
        let beat = rumble(noise, 0.14, 1100.0, 34.0);
        let gain = 0.45 + 0.1 * k as f32;
        mix(&mut track, at, &beat, gain * 3.0);
        mix(&mut track, at, &thump(0.14, 130.0, 80.0, 30.0), gain);
    }
    track
}

fn broadside(noise: &mut Noise) -> Vec<f32> {
    let mut track = vec![0.0; (1.6 * RATE as f32) as usize];
    for (at, pitch, gain) in [(0.0, 1.0, 1.0), (0.17, 1.15, 0.85), (0.33, 0.9, 0.95)] {
        let shot = cannon(noise, 1.0, pitch);
        mix(&mut track, at, &shot, gain);
    }
    mix(&mut track, 0.2, &rumble(noise, 1.4, 250.0, 2.2), 1.2);
    track
}

fn hit(noise: &mut Noise) -> Vec<f32> {
    let mut track = vec![0.0; (0.32 * RATE as f32) as usize];
    mix(&mut track, 0.0, &rumble(noise, 0.3, 2600.0, 26.0), 2.0);
    mix(&mut track, 0.0, &thump(0.3, 220.0, 70.0, 15.0), 0.9);
    track
}

fn fall(noise: &mut Noise) -> Vec<f32> {
    let mut track = vec![0.0; (0.95 * RATE as f32) as usize];
    // Cloth and gear shifting as the figure topples, then the ground.
    let mut low = LowPass::new(1500.0);
    let rustle = render(0.5, |t| {
        let swell = (t / 0.5 * PI).sin().powi(2);
        let n = noise.next();
        (n - low.filter(n)) * swell * 0.35
    });
    mix(&mut track, 0.0, &rustle, 1.0);
    mix(&mut track, 0.5, &thump(0.45, 90.0, 42.0, 11.0), 1.0);
    mix(&mut track, 0.5, &rumble(noise, 0.3, 700.0, 18.0), 1.4);
    track
}

fn sink(noise: &mut Noise) -> Vec<f32> {
    let seconds = 1.9;
    let mut track = vec![0.0; (seconds * RATE as f32) as usize];
    // The hull groaning downward as it goes under.
    let mut tone = glide(190.0, 38.0, 1.6);
    let groan = render(seconds, |t| {
        let swell = (t / seconds * PI).sin();
        tone(t) * swell * swell * 0.45
    });
    mix(&mut track, 0.0, &groan, 1.0);
    // Water rushing in, then closing over.
    let mut wash = LowPass::new(520.0);
    let water = render(seconds, |t| {
        let swell = (t / seconds * PI).sin().powf(0.7);
        wash.filter(noise.next()) * swell * (1.0 - 0.5 * t / seconds)
    });
    mix(&mut track, 0.0, &water, 3.2);
    // Bubbles rising.
    for _ in 0..16 {
        let at = noise.between(0.3, 1.6);
        let (hz, gain) = (noise.between(280.0, 900.0), noise.between(0.1, 0.3));
        let mut tone = glide(hz, hz * 1.3, 40.0);
        let bubble = render(0.09, |t| tone(t) * (-t * 55.0).exp());
        mix(&mut track, at, &bubble, gain);
    }
    track
}

fn surrender() -> Vec<f32> {
    let mut track = vec![0.0; (0.95 * RATE as f32) as usize];
    // Two soft falling notes: a bugle's retreat, without the bugle.
    for (at, hz) in [(0.0, 392.0), (0.38, 293.7)] {
        let note = render(0.55, |t| {
            let attack = (t / 0.02).min(1.0);
            let tone = (2.0 * PI * hz * t).sin() + 0.35 * (4.0 * PI * hz * t).sin();
            tone * attack * (-t * 4.5).exp()
        });
        mix(&mut track, at, &note, 0.6);
    }
    track
}

/// Scales a sound so its loudest sample reaches `peak`, with a few milliseconds of fade at both
/// ends so it starts and stops without a click.
fn finish(mut samples: Vec<f32>, peak: f32) -> Vec<f32> {
    let loudest = samples.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
    let fade = (0.006 * RATE as f32) as usize;
    let last = samples.len();
    for (i, s) in samples.iter_mut().enumerate() {
        let edge = i.min(last - 1 - i);
        let ramp = (edge as f32 / fade as f32).min(1.0);
        *s *= peak / loudest * ramp;
    }
    samples
}

/// Every combat cue by pack file name.
fn combat_sounds() -> Vec<(&'static str, Vec<f32>)> {
    let mut noise = Noise(0x9E37_79B9);
    vec![
        ("volley", finish(volley(&mut noise), 0.55)),
        ("charge", finish(charge(&mut noise), 0.5)),
        ("gun", finish(cannon(&mut noise, 1.1, 1.0), 0.65)),
        ("broadside", finish(broadside(&mut noise), 0.7)),
        ("hit", finish(hit(&mut noise), 0.6)),
        ("fall", finish(fall(&mut noise), 0.5)),
        ("sink", finish(sink(&mut noise), 0.55)),
        ("yield", finish(surrender(), 0.4)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(samples: &[f32]) -> f32 {
        samples.len() as f32 / RATE as f32
    }

    #[test]
    fn every_cue_the_pack_names_is_made() {
        let names: Vec<_> = combat_sounds().into_iter().map(|(name, _)| name).collect();
        assert_eq!(
            names,
            [
                "volley",
                "charge",
                "gun",
                "broadside",
                "hit",
                "fall",
                "sink",
                "yield"
            ]
        );
    }

    #[test]
    fn sounds_are_audible_but_never_clip() {
        for (name, samples) in combat_sounds() {
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!((0.35..=0.75).contains(&peak), "{name}: peak {peak}");
            let energy = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
            assert!(energy.sqrt() > 0.02, "{name} is nearly silent");
            assert!(samples.iter().all(|s| s.is_finite()), "{name}");
        }
    }

    #[test]
    fn sounds_start_and_stop_without_a_click() {
        for (name, samples) in combat_sounds() {
            assert!(samples[0].abs() < 0.01, "{name} starts with a click");
            assert!(
                samples[samples.len() - 1].abs() < 0.01,
                "{name} ends with a click"
            );
        }
    }

    #[test]
    fn lengths_suit_the_motions_they_accompany() {
        // Bounds follow the motion lengths of docs/combat-animation.md §7.
        let bounds = [
            ("volley", 0.4, 0.8),
            ("charge", 0.7, 1.1),
            ("gun", 0.8, 1.4),
            ("broadside", 1.2, 2.0),
            ("hit", 0.15, 0.5),
            ("fall", 0.7, 1.1),
            ("sink", 1.5, 2.2),
            ("yield", 0.7, 1.1),
        ];
        let sounds = combat_sounds();
        for (name, low, high) in bounds {
            let samples = &sounds.iter().find(|(n, _)| *n == name).unwrap().1;
            let length = seconds(samples);
            assert!((low..=high).contains(&length), "{name}: {length} s");
        }
    }

    #[test]
    fn generation_is_repeatable() {
        assert_eq!(combat_sounds(), combat_sounds());
    }
}
