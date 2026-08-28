#![allow(unused)]

use std::{sync::Arc, time::Duration};

use eyre::Result;
use cpal::{
    Error, InputCallbackInfo, OutputCallbackInfo, Sample, SampleFormat, Stream, StreamConfig,
    platform::{Device, Host},
    traits::{DeviceTrait, HostTrait},
};
use ringbuf::{
    HeapRb, SharedRb,
    traits::{Consumer, Producer, Split, SplitRef},
    wrap::caching::Caching,
};

pub struct InputDevice {
    pub device: Device,
    pub config: cpal::StreamConfig,
}
pub struct OutputDevice {
    pub device: Device,
    pub config: cpal::StreamConfig,
}

pub struct SystemSettings {
    pub host: Host,
    pub input_device: Option<InputDevice>,
    pub output_device: Option<OutputDevice>,
}

const TIMEOUT_DURATION: Duration = Duration::from_secs(60);
pub const SAMPLE_FORMAT: SampleFormat = SampleFormat::I16;
pub type SampleFormatType = i16;

impl SystemSettings {
    pub fn try_default() -> Result<Self> {
        let host = cpal::default_host();
        let input_device = host.default_input_device();
        let output_device = host.default_output_device();
        Self::new(host, input_device, output_device)
    }

    pub fn new(host: Host, input: Option<Device>, output: Option<Device>) -> Result<Self> {
        let input_device = if let Some(device) = input {
            let config = device
                .supported_input_configs()?
                .find(|c| c.sample_format() == SAMPLE_FORMAT);
            config.map(|config| InputDevice {
                config: config.with_max_sample_rate().into(),
                device,
            })
        } else {
            None
        };
        let output_device = if let Some(device) = output {
            let config = device
                .supported_output_configs()?
                .find(|c| c.sample_format() == SAMPLE_FORMAT);
            config.map(|config| OutputDevice {
                config: config.with_max_sample_rate().into(),
                device,
            })
        } else {
            None
        };
        Ok(Self {
            host,
            input_device,
            output_device,
        })
    }
}

pub struct AudioBuffer {
    latency: f32,
    config: StreamConfig,
    pub ring: Arc<HeapRb<SampleFormatType>>,
    pub producer: Caching<Arc<HeapRb<SampleFormatType>>, true, false>,
    pub consumer: Caching<Arc<HeapRb<SampleFormatType>>, false, true>,
}

impl AudioBuffer {
    pub fn new(latency: f32, config: StreamConfig) -> Self {
        let latency_frames = (latency / 1_000.0) * config.sample_rate as f32;
        let latency_samples = latency_frames as usize * config.channels as usize;
        let ring = Arc::new(HeapRb::<SampleFormatType>::new(latency_samples * 2)); // stream buffer
        let (mut producer, mut consumer) = ring.clone().split(); // split buffer for input and output
        // fill buffer with silence
        for _ in 0..latency_samples {
            producer.try_push(SampleFormatType::EQUILIBRIUM).unwrap();
        }
        Self {
            latency,
            config,
            ring,
            producer,
            consumer,
        }
    }
}

pub(crate) fn receive_audio(
    output: OutputDevice,
    mut consumer: impl Consumer<Item = SampleFormatType> + Send + 'static,
) -> Result<Stream, cpal::Error> {
    // TODO audio argument has to be audio stream / array of f32. Seperate functions for vc, screenshare and gui sounds?
    // feeding the streamed audio to the output
    let output_data_fn = move |data: &mut [SampleFormatType], _: &OutputCallbackInfo| {
        let read = consumer.pop_slice(data);
        if read < data.len() {
            data[read..].fill(SampleFormatType::EQUILIBRIUM); // insufficient streamed audio samples, replaced with silence
        }
    };
    output.device.build_output_stream(
        output.config,
        output_data_fn,
        err_fn,
        Some(TIMEOUT_DURATION),
    )
}

pub(crate) fn capture_audio(
    input: InputDevice,
    mut producer: impl Producer<Item = SampleFormatType> + Send + 'static,
) -> Result<Stream, cpal::Error> {
    // feeding the input audio into the stream buffer
    let input_data_fn = move |data: &[SampleFormatType], _: &InputCallbackInfo| {
        if producer.push_slice(data) < data.len() {
            // input audio is filling the buffer, increase buffer or streaming speed
        }
    };
    input
        .device
        .build_input_stream(input.config, input_data_fn, err_fn, Some(TIMEOUT_DURATION))
}

// error handling
fn err_fn(err: Error) {
    tracing::error!("{:?}", err);
    // TODO how should errors be displayed for a user
}
