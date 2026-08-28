/* TODO author, description
 * Speakrs - A communication client / server program
 * Copyright (C) 2026  Miranda Große-Heilmann
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/gpl-3.0>.
 */
use crate::common::config::{Config, config_home};
use clap::Parser;
use eyre::Result;
use std::{fmt::Debug, path::PathBuf};
use systemaudio::SystemSettings;

pub mod connection;
pub mod notifications;
pub mod repl;

mod systemaudio;
use cpal::traits::{DeviceTrait, StreamTrait};

#[derive(Debug, Parser)]
pub(crate) struct ClientArguments {
    /// With GUI, if false, runs TUI
    #[clap(long, default_value_t = false)]
    gui: bool,
}

pub(crate) async fn run(args: ClientArguments) -> Result<()> {
    if args.gui {
        gui(args)
    } else {
        repl::repl(args).await
    }
}

fn gui(_args: ClientArguments) -> Result<()> {
    tracing::info!("{:?}", audio_feedback_test()); // TODO replace with actual Sound Interface for vc, video, ui sounds
    speakrs_gui::run();
    return Ok(());
}

fn audio_feedback_test() -> Result<()> {
    // for testing audio input and output, TODO could be used for user settings
    let settings = systemaudio::SystemSettings::try_default()?;
    //if default audio device works, monitor mic
    if let SystemSettings {
        input_device: Some(input),
        output_device: Some(output),
        ..
    } = settings
    {
        tracing::info!("{:#?}", input.config);
        tracing::info!("{:#?}", output.config);
        tracing::info!("{:?}", output.device.id()?);
        tracing::info!("{:?}", input.device.id()?);
        let audio_buffer: systemaudio::AudioBuffer =
            systemaudio::AudioBuffer::new(50.0, input.config);
        let input_stream = systemaudio::capture_audio(input, audio_buffer.producer)?;
        let output_stream = systemaudio::receive_audio(output, audio_buffer.consumer)?;
        match input_stream.play() {
            Ok(_) => tracing::info!("audio input stream started"),
            Err(e) => tracing::warn!("audio input stream not started: {:?}", e),
        }
        match output_stream.play() {
            Ok(_) => tracing::info!("audio output stream started"),
            Err(e) => tracing::warn!("audio output stream not started: {:?}", e),
        }
        std::thread::sleep(std::time::Duration::from_secs(10));
    } else {
        tracing::warn!("Default Config not found");
    }
    return Ok(());
}

// ==============================
// => Config
// ==============================
// NOTE: For Devs: Try to annotate every value with `///` and explain what it does
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct ClientConfig {
    /// Database related settings
    database: ClientConfigDatabase,
}
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct ClientConfigDatabase {
    /// Directory to store client databases in, if empty stores databases next to config.
    /// If set to `/some/dir` creates `/some/dir/client` and `/some/dir/client/<uuid>` for each database.
    directory: Option<String>,
}
impl ClientConfig {
    /// See [`ClientConfig::database`]
    pub fn get_database_directory(&self) -> PathBuf {
        let path = self
            .database
            .directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| config_home().join("databases"));
        path.join("client")
    }
    /// Get ClientConfig from cached unified Config.
    /// This is a relative expensive operation (clones ClientConfig from R/W locked Config value), it might be deprecated in the future.
    /// TODO: currently throws an error if config does not have a client section
    pub fn get() -> Self {
        Config::clone_client().expect("Running client requires config to have client section")
    }
}
