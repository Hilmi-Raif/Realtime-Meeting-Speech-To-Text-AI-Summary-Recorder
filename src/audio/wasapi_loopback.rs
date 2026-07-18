use std::collections::VecDeque;
use std::thread;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use wasapi::{
    initialize_mta, Device, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat,
};

const LOOPBACK_SAMPLE_RATE: u32 = 48_000;
const LOOPBACK_CHANNELS: usize = 2;
const CHUNK_BYTES: usize = 4096;
const MAX_CONSECUTIVE_READ_ERRORS: usize = 10;
pub(crate) const DEFAULT_DEVICE_NAME: &str = "Default";

#[derive(Clone, Debug)]
pub enum WasapiCaptureEvent {
    SourceStarted {
        source_id: usize,
        label: String,
    },
    SourceFailed {
        source_id: usize,
        label: String,
        error: String,
    },
}

pub fn list_render_devices() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let _ = initialize_mta().ok();
    let enumerator = DeviceEnumerator::new()?;
    let collection = enumerator.get_device_collection(&Direction::Render)?;
    let mut devices = Vec::new();

    for device in &collection {
        match device.and_then(|d| d.get_friendlyname()) {
            Ok(name) => devices.push(name),
            Err(e) => warn!("Failed to read output device name: {}", e),
        }
    }

    Ok(devices)
}

fn get_render_device(device_name: &str) -> Result<Device, Box<dyn std::error::Error>> {
    let enumerator = DeviceEnumerator::new()?;

    if device_name == DEFAULT_DEVICE_NAME {
        info!("Using default WASAPI output device.");
        return Ok(enumerator.get_default_device(&Direction::Render)?);
    }

    if device_name.trim().is_empty() {
        return Err("WASAPI output device name is empty".into());
    }

    let needle = device_name.to_lowercase();
    for device in &enumerator.get_device_collection(&Direction::Render)? {
        let device = device?;
        let name = device.get_friendlyname()?;
        if name.to_lowercase().contains(&needle) {
            return Ok(device);
        }
    }

    Err(format!(
        "Failed to find WASAPI output device containing: {}",
        device_name
    )
    .into())
}

pub fn list_capture_devices() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let _ = initialize_mta().ok();
    let enumerator = DeviceEnumerator::new()?;
    let collection = enumerator.get_device_collection(&Direction::Capture)?;
    let mut devices = Vec::new();

    for device in &collection {
        match device.and_then(|d| d.get_friendlyname()) {
            Ok(name) => devices.push(name),
            Err(e) => warn!("Failed to read capture device name: {}", e),
        }
    }

    Ok(devices)
}

fn get_capture_device(device_name: &str) -> Result<Device, Box<dyn std::error::Error>> {
    let enumerator = DeviceEnumerator::new()?;

    if device_name == DEFAULT_DEVICE_NAME {
        info!("Using default WASAPI capture device.");
        return Ok(enumerator.get_default_device(&Direction::Capture)?);
    }

    if device_name.trim().is_empty() {
        return Err("WASAPI capture device name is empty".into());
    }

    let needle = device_name.to_lowercase();
    for device in &enumerator.get_device_collection(&Direction::Capture)? {
        let device = device?;
        let name = device.get_friendlyname()?;
        if name.to_lowercase().contains(&needle) {
            return Ok(device);
        }
    }

    Err(format!(
        "Failed to find WASAPI capture device containing: {}",
        device_name
    )
    .into())
}

pub fn start_wasapi_loopback_capture(
    render_device_names: Vec<String>,
    capture_device_names: Vec<String>,
    audio_tx: mpsc::Sender<Vec<u8>>,
    record_tx: mpsc::UnboundedSender<Vec<u8>>,
    event_tx: std::sync::mpsc::Sender<WasapiCaptureEvent>,
    cancel_token: CancellationToken,
) -> Result<u32, Box<dyn std::error::Error>> {
    let mut source_id = 0;
    let (source_tx, source_rx) = std::sync::mpsc::channel();
    let (startup_tx, startup_rx) = std::sync::mpsc::channel();

    let (names_to_init_render, names_to_init_capture) =
        selected_audio_sources(render_device_names, capture_device_names)?;

    for name in names_to_init_render {
        spawn_device_thread(
            name,
            Direction::Render,
            source_id,
            source_tx.clone(),
            event_tx.clone(),
            startup_tx.clone(),
            cancel_token.clone(),
        )?;
        source_id += 1;
    }

    for name in names_to_init_capture {
        spawn_device_thread(
            name,
            Direction::Capture,
            source_id,
            source_tx.clone(),
            event_tx.clone(),
            startup_tx.clone(),
            cancel_token.clone(),
        )?;
        source_id += 1;
    }

    let total_sources = source_id;
    wait_for_sources_started(total_sources, startup_rx, cancel_token.clone())?;

    thread::Builder::new()
        .name("wasapi-mixer".to_string())
        .spawn(move || {
            run_async_mixer_loop(source_rx, audio_tx, record_tx, total_sources, cancel_token);
        })?;

    Ok(LOOPBACK_SAMPLE_RATE)
}

fn selected_audio_sources(
    render_device_names: Vec<String>,
    capture_device_names: Vec<String>,
) -> Result<(Vec<String>, Vec<String>), String> {
    if render_device_names.is_empty() && capture_device_names.is_empty() {
        return Err("Select at least one microphone or system audio device.".to_string());
    }

    Ok((render_device_names, capture_device_names))
}

fn spawn_device_thread(
    device_name: String,
    direction: Direction,
    source_id: usize,
    tx: std::sync::mpsc::Sender<SourceChunk>,
    event_tx: std::sync::mpsc::Sender<WasapiCaptureEvent>,
    startup_tx: std::sync::mpsc::Sender<WasapiCaptureEvent>,
    cancel_token: CancellationToken,
) -> Result<(), std::io::Error> {
    let label = source_label(direction, &device_name);
    thread::Builder::new()
        .name(format!("wasapi-device-{}", source_id))
        .spawn(move || {
            match device_worker_loop(
                &device_name,
                direction,
                source_id,
                tx,
                event_tx.clone(),
                startup_tx.clone(),
                cancel_token,
            ) {
                Ok(()) => {}
                Err(e) => {
                    let error = e.to_string();
                    error!("WASAPI worker {} failed: {}", source_id, error);
                    let event = WasapiCaptureEvent::SourceFailed {
                        source_id,
                        label,
                        error,
                    };
                    let _ = startup_tx.send(event.clone());
                    let _ = event_tx.send(event);
                }
            }
        })?;
    Ok(())
}

fn device_worker_loop(
    device_name: &str,
    direction: Direction,
    source_id: usize,
    tx: std::sync::mpsc::Sender<SourceChunk>,
    event_tx: std::sync::mpsc::Sender<WasapiCaptureEvent>,
    startup_tx: std::sync::mpsc::Sender<WasapiCaptureEvent>,
    cancel_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error>> {
    initialize_mta().ok()?;

    let device = if direction == Direction::Render {
        get_render_device(device_name)?
    } else {
        get_capture_device(device_name)?
    };

    let mut audio_client = device.get_iaudioclient()?;
    let desired_format = WaveFormat::new(
        32,
        32,
        &SampleType::Float,
        LOOPBACK_SAMPLE_RATE as usize,
        LOOPBACK_CHANNELS,
        None,
    );

    let stream_dir = Direction::Capture;

    let mut is_event = true;
    let (_, min_time) = audio_client.get_device_period().unwrap_or((0, 100_000));
    let mode_event = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: min_time.max(200_000),
    };

    let event_handle = if audio_client
        .initialize_client(&desired_format, &stream_dir, &mode_event)
        .is_ok()
    {
        Some(audio_client.set_get_eventhandle()?)
    } else {
        // fall back to polling when event driven capture is unavailable
        is_event = false;
        let mut new_client = device.get_iaudioclient()?;
        let mode_poll = StreamMode::PollingShared {
            autoconvert: true,
            buffer_duration_hns: 1_000_000,
        };
        new_client.initialize_client(&desired_format, &stream_dir, &mode_poll)?;
        audio_client = new_client;
        None
    };

    let capture_client = audio_client.get_audiocaptureclient()?;
    audio_client.start_stream()?;
    let friendly_name = device.get_friendlyname()?;
    info!(
        "Started WASAPI capture for source {} ({}), event_driven: {}",
        source_id, friendly_name, is_event
    );
    let event = WasapiCaptureEvent::SourceStarted {
        source_id,
        label: format!("{}: {}", direction_label(direction), friendly_name),
    };
    let _ = startup_tx.send(event.clone());
    let _ = event_tx.send(event);

    let block_align = desired_format.get_blockalign() as usize;
    let mut raw_queue = VecDeque::<u8>::with_capacity(block_align * LOOPBACK_SAMPLE_RATE as usize);
    let mut consecutive_read_errors = 0;

    while !cancel_token.is_cancelled() {
        if is_event {
            if let Err(e) = capture_client.read_from_device_to_deque(&mut raw_queue) {
                warn!("WASAPI event read failed on source {}: {}", source_id, e);
                consecutive_read_errors += 1;
                if consecutive_read_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                    return Err(format!(
                        "WASAPI source {} failed after {} read errors: {}",
                        source_id, consecutive_read_errors, e
                    )
                    .into());
                }
            } else {
                consecutive_read_errors = 0;
            }
        } else {
            loop {
                let frames = match capture_client.get_next_packet_size() {
                    Ok(Some(f)) => f,
                    Ok(None) => break,
                    Err(e) => {
                        warn!(
                            "WASAPI packet size read failed on source {}: {}",
                            source_id, e
                        );
                        consecutive_read_errors += 1;
                        if consecutive_read_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                            return Err(format!(
                                "WASAPI source {} failed after {} packet read errors: {}",
                                source_id, consecutive_read_errors, e
                            )
                            .into());
                        }
                        break;
                    }
                };
                if frames == 0 {
                    break;
                }
                let bytes_needed = frames as usize * LOOPBACK_CHANNELS * std::mem::size_of::<f32>();
                let mut temp_buf = vec![0u8; bytes_needed];
                match capture_client.read_from_device(&mut temp_buf) {
                    Ok((frames_read, info)) => {
                        consecutive_read_errors = 0;
                        let read_bytes =
                            frames_read as usize * LOOPBACK_CHANNELS * std::mem::size_of::<f32>();
                        if info.flags.silent {
                            raw_queue.extend(std::iter::repeat_n(0, read_bytes));
                        } else {
                            raw_queue.extend(&temp_buf[..read_bytes]);
                        }
                    }
                    Err(e) => {
                        warn!("WASAPI polling read failed on source {}: {}", source_id, e);
                        consecutive_read_errors += 1;
                        if consecutive_read_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                            return Err(format!(
                                "WASAPI source {} failed after {} polling read errors: {}",
                                source_id, consecutive_read_errors, e
                            )
                            .into());
                        }
                        break;
                    }
                }
            }
        }

        let mut i16_mono = Vec::new();
        let frame_bytes = LOOPBACK_CHANNELS * std::mem::size_of::<f32>();
        while raw_queue.len() >= frame_bytes {
            let left = pop_f32_le(&mut raw_queue);
            let right = pop_f32_le(&mut raw_queue);
            let mono = ((left + right) / 2.0).clamp(-1.0, 1.0);
            i16_mono.push((mono * i16::MAX as f32) as i16);
        }

        if !i16_mono.is_empty()
            && tx
                .send(SourceChunk {
                    source_id,
                    data: i16_mono,
                })
                .is_err()
        {
            break;
        }

        if is_event {
            if let Some(h) = &event_handle {
                if h.wait_for_event(100).is_err() {
                    thread::sleep(Duration::from_millis(5));
                }
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }

    let _ = audio_client.stop_stream();
    info!("Stopped WASAPI capture for source {}", source_id);
    Ok(())
}

fn wait_for_sources_started(
    source_count: usize,
    startup_rx: std::sync::mpsc::Receiver<WasapiCaptureEvent>,
    cancel_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut started = 0;

    while started < source_count {
        let now = std::time::Instant::now();
        if now >= deadline {
            cancel_token.cancel();
            return Err(format!(
                "Audio startup timed out: {}/{} WASAPI sources started",
                started, source_count
            )
            .into());
        }

        match startup_rx.recv_timeout(deadline.saturating_duration_since(now)) {
            Ok(WasapiCaptureEvent::SourceStarted { .. }) => started += 1,
            Ok(WasapiCaptureEvent::SourceFailed { label, error, .. }) => {
                cancel_token.cancel();
                return Err(format!("Audio source failed ({label}): {error}").into());
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                cancel_token.cancel();
                return Err(format!(
                    "Audio startup timed out: {}/{} WASAPI sources started",
                    started, source_count
                )
                .into());
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                cancel_token.cancel();
                return Err("Audio startup channel closed before sources were ready".into());
            }
        }
    }

    Ok(())
}

fn source_label(direction: Direction, device_name: &str) -> String {
    let device = if device_name.trim().is_empty() {
        "default device"
    } else {
        device_name.trim()
    };
    format!("{}: {}", direction_label(direction), device)
}

fn direction_label(direction: Direction) -> &'static str {
    match direction {
        Direction::Render => "output",
        Direction::Capture => "input",
    }
}

fn publish_pcm_chunk(
    pcm_chunk: &mut Vec<u8>,
    audio_tx: &mpsc::Sender<Vec<u8>>,
    record_tx: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let chunk = std::mem::take(pcm_chunk);
    let _ = record_tx.send(chunk.clone());

    if let Err(mpsc::error::TrySendError::Full(_)) = audio_tx.try_send(chunk) {
        warn!("Deepgram audio queue full; dropping realtime chunk to keep WAV capture smooth");
    }
}

fn pop_f32_le(raw_queue: &mut VecDeque<u8>) -> f32 {
    let bytes = [
        raw_queue.pop_front().unwrap_or_default(),
        raw_queue.pop_front().unwrap_or_default(),
        raw_queue.pop_front().unwrap_or_default(),
        raw_queue.pop_front().unwrap_or_default(),
    ];
    f32::from_le_bytes(bytes)
}

struct SourceChunk {
    source_id: usize,
    data: Vec<i16>,
}

fn run_async_mixer_loop(
    source_rx: std::sync::mpsc::Receiver<SourceChunk>,
    audio_tx: mpsc::Sender<Vec<u8>>,
    record_tx: mpsc::UnboundedSender<Vec<u8>>,
    source_count: usize,
    cancel_token: CancellationToken,
) {
    let mut queues: Vec<VecDeque<i16>> = vec![VecDeque::new(); source_count];
    let mut last_seen = vec![std::time::Instant::now(); source_count];
    let mut pcm_chunk = Vec::<u8>::with_capacity(CHUNK_BYTES);

    let mix_threshold_frames = 2400;
    let max_buffer_frames = 19200;

    while !cancel_token.is_cancelled() {
        if let Ok(chunk) = source_rx.recv_timeout(std::time::Duration::from_millis(10)) {
            if chunk.source_id < source_count {
                queues[chunk.source_id].extend(chunk.data);
                last_seen[chunk.source_id] = std::time::Instant::now();

                // cap buffers to avoid unbounded growth when source clocks drift
                while queues[chunk.source_id].len() > max_buffer_frames {
                    queues[chunk.source_id].pop_front();
                }
            }

            while let Ok(chunk) = source_rx.try_recv() {
                if chunk.source_id < source_count {
                    queues[chunk.source_id].extend(chunk.data);
                    last_seen[chunk.source_id] = std::time::Instant::now();
                    while queues[chunk.source_id].len() > max_buffer_frames {
                        queues[chunk.source_id].pop_front();
                    }
                }
            }
        }

        let now = std::time::Instant::now();
        let mut active_indices = Vec::new();
        for (i, seen_at) in last_seen.iter().enumerate().take(source_count) {
            if now.duration_since(*seen_at).as_millis() < 200 {
                active_indices.push(i);
            }
        }

        if active_indices.is_empty() {
            continue;
        }

        let mut min_frames = usize::MAX;
        for &i in &active_indices {
            min_frames = std::cmp::min(min_frames, queues[i].len());
        }

        // wait for a small jitter buffer before mixing multiple async sources
        if min_frames >= mix_threshold_frames || (min_frames > 0 && cancel_token.is_cancelled()) {
            for _ in 0..min_frames {
                let mut sum = 0.0_f32;
                for &i in &active_indices {
                    let sample = queues[i].pop_front().unwrap_or(0);
                    sum += sample as f32;
                }

                let mono_f32 = sum.clamp(i16::MIN as f32, i16::MAX as f32);
                let sample = mono_f32 as i16;
                pcm_chunk.extend_from_slice(&sample.to_le_bytes());

                if pcm_chunk.len() >= CHUNK_BYTES {
                    publish_pcm_chunk(&mut pcm_chunk, &audio_tx, &record_tx);
                }
            }
        }
    }

    if !pcm_chunk.is_empty() {
        publish_pcm_chunk(&mut pcm_chunk, &audio_tx, &record_tx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_audio_sources_skip_empty_sides() {
        let (render_sources, capture_sources) =
            selected_audio_sources(vec!["Speaker".to_string()], Vec::new()).unwrap();

        assert_eq!(render_sources, vec!["Speaker".to_string()]);
        assert!(capture_sources.is_empty());
    }

    #[test]
    fn selected_audio_sources_keep_explicit_default_selection() {
        let (render_sources, capture_sources) = selected_audio_sources(
            vec![DEFAULT_DEVICE_NAME.to_string()],
            vec![DEFAULT_DEVICE_NAME.to_string()],
        )
        .unwrap();

        assert_eq!(render_sources, vec![DEFAULT_DEVICE_NAME.to_string()]);
        assert_eq!(capture_sources, vec![DEFAULT_DEVICE_NAME.to_string()]);
    }

    #[test]
    fn selected_audio_sources_reject_empty_input_and_output() {
        let error = selected_audio_sources(Vec::new(), Vec::new()).unwrap_err();

        assert_eq!(
            error,
            "Select at least one microphone or system audio device."
        );
    }

    #[tokio::test]
    async fn publish_pcm_chunk_keeps_wav_chunk_when_realtime_queue_is_full() {
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(1);
        audio_tx.try_send(vec![1, 2]).unwrap();
        let (record_tx, mut record_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let mut pcm_chunk = vec![3, 4, 5, 6];

        publish_pcm_chunk(&mut pcm_chunk, &audio_tx, &record_tx);

        assert!(pcm_chunk.is_empty());
        assert_eq!(record_rx.recv().await, Some(vec![3, 4, 5, 6]));
        assert_eq!(audio_rx.recv().await, Some(vec![1, 2]));
        assert!(audio_rx.try_recv().is_err());
    }

    #[test]
    fn mixer_thread_aggregates_multiple_sources() {
        use std::sync::mpsc as std_mpsc;
        let (source_tx, source_rx) = std_mpsc::channel();
        let (audio_tx, _audio_rx) = mpsc::channel::<Vec<u8>>(10);
        let (record_tx, mut record_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let cancel_token = CancellationToken::new();

        let cancel_token_clone = cancel_token.clone();

        std::thread::spawn(move || {
            run_async_mixer_loop(source_rx, audio_tx, record_tx, 2, cancel_token_clone);
        });

        source_tx
            .send(SourceChunk {
                source_id: 0,
                data: vec![100, 200, 300, 400],
            })
            .unwrap();
        source_tx
            .send(SourceChunk {
                source_id: 1,
                data: vec![50, 100, 150, 200],
            })
            .unwrap();

        let expected_i16 = vec![150i16, 300, 450, 600];
        let mut expected_bytes = Vec::new();
        for sample in expected_i16 {
            expected_bytes.extend_from_slice(&sample.to_le_bytes());
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
        cancel_token.cancel();

        let out_record = record_rx.blocking_recv().unwrap();
        assert_eq!(out_record, expected_bytes);
    }
}
