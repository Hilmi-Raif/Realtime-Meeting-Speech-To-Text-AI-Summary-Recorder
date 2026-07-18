use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct AudioActivityConfig {
    pub window_ms: u32,
    pub silence_threshold_dbfs: f64,
    pub min_active_ms: u32,
}

impl Default for AudioActivityConfig {
    fn default() -> Self {
        Self {
            window_ms: 100,
            silence_threshold_dbfs: -50.0,
            min_active_ms: 300,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AudioActivity {
    pub duration_ms: u64,
    pub active_ms: u64,
    pub peak_amplitude: i16,
    pub rms_dbfs: f64,
    pub is_active: bool,
}

pub fn analyze_wav_activity(
    path: impl AsRef<Path>,
    config: AudioActivityConfig,
) -> Result<AudioActivity, String> {
    let path = path.as_ref();
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| format!("Failed to read WAV {}: {e}", path.display()))?;
    let spec = reader.spec();

    if spec.sample_rate == 0 || spec.channels == 0 {
        return Err("WAV sample rate/channel invalid".to_string());
    }
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err(format!(
            "Unsupported WAV format: {:?} {}-bit",
            spec.sample_format, spec.bits_per_sample
        ));
    }

    let channels = spec.channels as u64;
    let window_ms = config.window_ms.max(1) as u64;
    let window_samples = ((spec.sample_rate as u64 * channels * window_ms) / 1_000).max(1);

    let mut total_samples = 0_u64;
    let mut total_square_sum = 0_f64;
    let mut peak_amplitude = 0_i16;
    let mut active_ms = 0_u64;

    let mut window_sample_count = 0_u64;
    let mut window_square_sum = 0_f64;

    for sample in reader.samples::<i16>() {
        let sample = sample.map_err(|e| format!("Failed to read WAV sample: {e}"))?;
        let abs = (sample as i32).abs().min(i16::MAX as i32) as i16;
        peak_amplitude = peak_amplitude.max(abs);

        let sample_f64 = sample as f64;
        let square = sample_f64 * sample_f64;
        total_square_sum += square;
        window_square_sum += square;
        total_samples += 1;
        window_sample_count += 1;

        if window_sample_count >= window_samples {
            if is_active_window(
                window_square_sum,
                window_sample_count,
                config.silence_threshold_dbfs,
            ) {
                active_ms += samples_to_ms(window_sample_count, channels, spec.sample_rate);
            }
            window_sample_count = 0;
            window_square_sum = 0.0;
        }
    }

    if window_sample_count > 0
        && is_active_window(
            window_square_sum,
            window_sample_count,
            config.silence_threshold_dbfs,
        )
    {
        active_ms += samples_to_ms(window_sample_count, channels, spec.sample_rate);
    }

    let duration_ms = samples_to_ms(total_samples, channels, spec.sample_rate);
    let rms_dbfs = dbfs_from_square_sum(total_square_sum, total_samples);

    Ok(AudioActivity {
        duration_ms,
        active_ms,
        peak_amplitude,
        rms_dbfs,
        is_active: active_ms >= config.min_active_ms as u64,
    })
}

fn is_active_window(square_sum: f64, sample_count: u64, threshold_dbfs: f64) -> bool {
    dbfs_from_square_sum(square_sum, sample_count) >= threshold_dbfs
}

fn dbfs_from_square_sum(square_sum: f64, sample_count: u64) -> f64 {
    if sample_count == 0 || square_sum <= 0.0 {
        return f64::NEG_INFINITY;
    }

    let rms = (square_sum / sample_count as f64).sqrt();
    if rms <= 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * (rms / i16::MAX as f64).log10()
    }
}

fn samples_to_ms(samples: u64, channels: u64, sample_rate: u32) -> u64 {
    if channels == 0 || sample_rate == 0 {
        return 0;
    }

    samples.saturating_mul(1_000) / channels / sample_rate as u64
}

#[cfg(test)]
mod tests {
    use super::{analyze_wav_activity, AudioActivityConfig};
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SAMPLE_RATE: u32 = 16_000;

    #[test]
    fn silent_wav_is_inactive_even_when_file_has_duration() {
        let path = write_wav("silent", vec![0; SAMPLE_RATE as usize]);

        let activity = analyze_wav_activity(&path, AudioActivityConfig::default()).unwrap();

        assert!(!activity.is_active);
        assert_eq!(activity.active_ms, 0);
        assert!(activity.duration_ms >= 900);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn sustained_voice_level_audio_is_active() {
        let path = write_wav("voice", vec![2_000; SAMPLE_RATE as usize]);

        let activity = analyze_wav_activity(&path, AudioActivityConfig::default()).unwrap();

        assert!(activity.is_active);
        assert!(activity.active_ms >= 300);
        assert!(activity.rms_dbfs > -50.0);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn low_noise_below_threshold_is_inactive() {
        let path = write_wav("noise", vec![20; SAMPLE_RATE as usize]);

        let activity = analyze_wav_activity(&path, AudioActivityConfig::default()).unwrap();

        assert!(!activity.is_active);
        assert!(activity.rms_dbfs < -50.0);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn isolated_click_does_not_count_as_valid_audio() {
        let mut samples = vec![0; SAMPLE_RATE as usize];
        samples[0] = i16::MAX;
        let path = write_wav("click", samples);

        let activity = analyze_wav_activity(&path, AudioActivityConfig::default()).unwrap();

        assert!(!activity.is_active);
        assert!(activity.peak_amplitude > 0);

        let _ = fs::remove_file(path);
    }

    fn write_wav(name: &str, samples: Vec<i16>) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "rms_audio_analysis_{}_{}_{}.wav",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let spec = WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&path, spec).unwrap();
        for sample in samples {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        path
    }
}
