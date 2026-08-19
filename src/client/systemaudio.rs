use cpal::{
    Error, InputCallbackInfo, OutputCallbackInfo, Sample, Stream, StreamConfig, traits::{DeviceTrait, HostTrait}
};
use ringbuf::{
    HeapRb,
    traits::{Consumer, Producer}
};

pub struct SystemSettings {
    host: cpal::platform::Host,
    pub output_device: Option<cpal::platform::Device>,
    input_device: Option<cpal::platform::Device>,
    pub output_config: Option<cpal::StreamConfig>,
    pub input_config: Option<cpal::StreamConfig>,
}

impl Default for SystemSettings {
    fn default() -> Self {
        let output_device = cpal::default_host().default_output_device();
        let input_device = cpal::default_host().default_input_device();
        let output_config = match output_device {
            Some(v) => {
                match v.default_output_config() {
                    Ok(x) => Some(x.into()),
                    Err(_) => None
                }
            },
            None => None
        };
        let input_config = match input_device {
            Some(v) => {
                match v.default_input_config() {
                    Ok(x) => Some(x.into()),
                    Err(_) => None
                }
            },
            None => None
        };
        Self {
            host: cpal::default_host(),
            output_device: cpal::default_host().default_output_device(),
            input_device: cpal::default_host().default_input_device(),
            output_config,
            input_config,
        }
    }
}

pub struct AudioBuffer {
    latency: f32,
    config: StreamConfig,
    pub ring: HeapRb<f32>,
}

impl AudioBuffer {
    pub fn new(latency: f32, config: StreamConfig) -> Self {
        let latency_frames = (latency / 1_000.0) * config.sample_rate as f32;
        let latency_samples = latency_frames as usize * config.channels as usize;
        let ring = HeapRb::<f32>::new(latency_samples * 2);
        Self {
            latency,
            config,
            ring,
        }
    }

}

pub(crate) fn receive_audio(sys: SystemSettings, mut consumer: impl Consumer<Item = f32> + Send + 'static) -> Option<Stream> { // TODO audio argument has to be audio stream / array of f32. Seperate functions for vc, screenshare and gui sounds?
    // feeding the streamed audio to the output
    let output_data_fn = move |data: &mut [f32], _: &OutputCallbackInfo| {
        let read = consumer.pop_slice(data);
        if read < data.len() {
            data[read..].fill(f32::EQUILIBRIUM); // insufficient streamed audio samples, replaced with silence
        }
    };
    let stream = match sys.output_device {
        Some(v) => {
            if sys.output_config.is_none() {
                None
            }
            else {
                match v.build_output_stream(sys.output_config.unwrap(), output_data_fn, err_fn, None) {
                    Ok(x) => Some(x),
                    Err(_) => None
                }
            }
        },
        None => None
    }; // TODO run "stream.play()?" somewhere, probably either client or here
    return stream;
}

pub(crate) fn capture_audio(sys: SystemSettings, mut producer: impl Producer<Item = f32> + Send + 'static) -> Option<Stream> {

    // feeding the input audio into the stream buffer
    let input_data_fn = move |data: &[f32], _: &InputCallbackInfo| {
        if producer.push_slice(data) < data.len() {
            // input audio is filling the buffer, increase buffer or streaming speed
        }
    };
    let stream = match sys.input_device {
        Some(v) => {
            if sys.input_config.is_none() {
                None
            }
            else {
                match v.build_input_stream(sys.input_config.unwrap(), input_data_fn, err_fn, None) {
                    Ok(x) => Some(x),
                    Err(_) => None
                }
            }
        },
        None => None
    }; // TODO run "stream.play()?" somewhere, probably either client or here
    return stream;
}

// error handling
fn err_fn(err: Error) {
    todo!() // TODO how should errors be displayed for a user
}
