use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use serde::Deserialize;

use log::{debug, info, trace};

use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::config::Config;
use crate::monitor::NginxMeta;
use crate::probe::VideoProbe;

#[derive(Debug)]
pub struct Encoder {
    id: String,
    path: PathBuf,
    config: EncodingConfiguration,

    video_width: usize,
    video_height: usize,
    video_framerate: f32,

    duration: f32,

    global_config: Config,
}

#[derive(Debug)]
pub struct EncoderResult {
    pub variants: Vec<(usize, String, String)>,
    pub duration: f32,
}

impl Encoder {
    pub async fn new(id: String, global_config: &Config) -> Result<Self, EncoderError> {
        let mut contents = vec![];

        let mut config_file = File::open("encoding.toml").await?;
        config_file.read_to_end(&mut contents).await?;

        let path = Path::new(&global_config.storage_dir)
            .join("recordings")
            .join(&id)
            .with_extension("flv");
        if !path.exists() {
            return Err(EncoderError::MissingInput);
        }

        let meta = VideoProbe::new(path.as_path())?.probe()?;
        let (video_width, video_height, video_framerate) = meta
            .iter()
            .filter_map(|a| match a {
                NginxMeta::Video {
                    width,
                    height,
                    frame_rate,
                    ..
                } => Some((*width, *height, *frame_rate)),
                _ => None,
            })
            .nth(0)
            .unwrap();
        let duration = meta
            .iter()
            .filter_map(|a| match a {
                NginxMeta::Format { duration, .. } => Some(*duration),
                _ => None,
            })
            .nth(0)
            .unwrap();

        let config = toml::from_slice(&contents)?;
        Ok(Encoder {
            id,
            config,
            path: path.to_path_buf(),

            video_width,
            video_height,
            video_framerate,

            duration,

            global_config: global_config.clone(),
        })
    }

    pub async fn encode(self) -> Result<EncoderResult, EncoderError> {
        info!("Encoding video `{}`", self.id);

        let mut variants = Vec::new();

        let output_dir = Path::new(&self.global_config.storage_dir)
            .join("encoded")
            .join(&self.id);
        if !output_dir.exists() {
            tokio::fs::create_dir_all(&output_dir).await?;
        }

        for (id, vp9_config) in self.config.vod.vp9 {
            if self.video_height < vp9_config.height {
                continue;
            }

            let fps = min(
                self.video_framerate,
                vp9_config.max_fps.unwrap_or(self.video_framerate),
            );

            let filename = format!("vp9_{}.webm", id);
            variants.push((vp9_config.height, "video/webm".into(), filename.clone()));
            let output = output_dir.join(filename);
            if output.exists() {
                debug!("VP9 {}. Skipping", id);
                continue;
            }

            let mut workdir = env::temp_dir();
            let rand_string: String = thread_rng().sample_iter(&Alphanumeric).take(30).collect();
            workdir.push(rand_string);
            tokio::fs::create_dir_all(&workdir).await?;

            debug!(
                "VP9 {} ({}): {} -> {}",
                id,
                workdir.display(),
                self.path.display(),
                output.display()
            );
            for mut cmd in VP9::commands(
                vp9_config.bitrate,
                vp9_config.height,
                fps,
                vp9_config.audio_channels,
                48000,
                &self.path,
                &output,
                &workdir,
            )? {
                trace!("{:?}", cmd);

                let _output = cmd.output().await?;
            }

            tokio::fs::remove_dir_all(&workdir).await?;
        }

        for (id, h264_config) in self.config.vod.h264 {
            if self.video_height < h264_config.height {
                continue;
            }

            let fps = min(
                self.video_framerate,
                h264_config.max_fps.unwrap_or(self.video_framerate),
            );

            let filename = format!("h264_{}.mp4", id);
            variants.push((h264_config.height, "video/mp4".into(), filename.clone()));
            let output = output_dir.join(filename);
            if output.exists() {
                debug!("H.264 {}. Skipping", id);
                continue;
            }

            let mut workdir = env::temp_dir();
            let rand_string: String = thread_rng().sample_iter(&Alphanumeric).take(30).collect();
            workdir.push(rand_string);
            tokio::fs::create_dir_all(&workdir).await?;

            debug!(
                "H.264 {} ({}): {} -> {}",
                id,
                workdir.display(),
                self.path.display(),
                output.display()
            );
            for mut cmd in H264::commands(
                h264_config.bitrate,
                h264_config.height,
                fps,
                h264_config.audio_channels,
                48000,
                &self.path,
                &output,
                &workdir,
            )? {
                trace!("{:?}", cmd);

                let _output = cmd.output().await?;
            }

            tokio::fs::remove_dir_all(&workdir).await?;
        }

        Ok(EncoderResult {
            variants,
            duration: self.duration,
        })
    }
}

trait Codec {
    fn commands(
        bitrate: usize,
        height: usize,
        fps: f32,
        audio_channels: usize,
        audio_sampling: usize,
        input: &Path,
        output: &Path,
        workdir: &Path,
    ) -> Result<Vec<Command>, EncoderError>;
}

struct VP9;
impl Codec for VP9 {
    fn commands(
        bitrate: usize,
        height: usize,
        fps: f32,
        audio_channels: usize,
        audio_sampling: usize,
        input: &Path,
        output: &Path,
        workdir: &Path,
    ) -> Result<Vec<Command>, EncoderError> {
        let mut command_1 = Command::new("ffmpeg");
        command_1
            .current_dir(workdir)
            .kill_on_drop(true)
            .arg("-y")
            .args(&["-i", input.to_str().unwrap()])
            .args(&["-vf", &format!("scale=-2:{}", height)])
            // .args(&["-filter:v", &format!("fps=fps={}", fps)]) TODO merge with scaling
            .args(&["-c:v", "libvpx-vp9"])
            .args(&["-b:v", &format!("{}K", bitrate)])
            .args(&["-threads", "4"])
            .args(&["-row-mt", "1"])
            .args(&["-pass", "1"])
            .args(&["-an"])
            .args(&["-f", "webm"])
            .args(&["/dev/null"]);
        let mut command_2 = Command::new("ffmpeg");
        command_2
            .current_dir(workdir)
            .kill_on_drop(true)
            .arg("-y")
            .args(&["-i", input.to_str().unwrap()])
            .args(&["-vf", &format!("scale=-2:{}", height)])
            // .args(&["-filter:v", &format!("fps=fps={}", fps)])
            .args(&["-c:v", "libvpx-vp9"])
            .args(&["-b:v", &format!("{}K", bitrate)])
            .args(&["-threads", "4"])
            .args(&["-row-mt", "1"])
            .args(&["-pass", "2"])
            .args(&["-c:a", "libopus"])
            .args(&["-ac", &format!("{}", audio_channels)])
            .args(&["-ar", &format!("{}", audio_sampling)])
            .args(&["-f", "webm"])
            .args(&[output.to_str().unwrap()]);

        Ok(vec![command_1, command_2])
    }
}

struct H264;
impl Codec for H264 {
    fn commands(
        bitrate: usize,
        height: usize,
        fps: f32,
        audio_channels: usize,
        audio_sampling: usize,
        input: &Path,
        output: &Path,
        workdir: &Path,
    ) -> Result<Vec<Command>, EncoderError> {
        let mut command = Command::new("ffmpeg");
        command
            .current_dir(workdir)
            .kill_on_drop(true)
            .arg("-y")
            .args(&["-i", input.to_str().unwrap()])
            .args(&["-vf", &format!("scale=-2:{}", height)])
            // .args(&["-filter:v", &format!("fps=fps={}", fps)])
            .args(&["-c:v", "libx264"])
            .args(&["-b:v", &format!("{}K", bitrate)])
            .args(&["-c:a", "aac"])
            .args(&["-ac", &format!("{}", audio_channels)])
            .args(&["-ar", &format!("{}", audio_sampling)])
            .args(&["-f", "mp4"])
            .args(&[output.to_str().unwrap()]);
        Ok(vec![command])
    }
}

#[derive(Debug, Deserialize)]
pub struct EncodingConfiguration {
    pub vod: EncodingCodecs,
}

#[derive(Debug, Deserialize)]
pub struct EncodingCodecs {
    #[serde(default)]
    pub vp9: HashMap<String, EncodingVariant>,

    #[serde(default)]
    pub h264: HashMap<String, EncodingVariant>,
}

#[derive(Debug, Deserialize)]
pub struct EncodingVariant {
    pub height: usize,
    pub bitrate: usize,
    pub audio_channels: usize,
    pub max_fps: Option<f32>,
}

fn min<T: PartialOrd>(a: T, b: T) -> T {
    if a < b {
        a
    } else {
        b
    }
}

#[derive(Debug)]
pub enum EncoderError {
    MissingInput,
    VideoProbe(crate::probe::VideoProbeError),

    TokioIO(tokio::io::Error),
    TOML(toml::de::Error),
}

impl From<tokio::io::Error> for EncoderError {
    fn from(other: tokio::io::Error) -> Self {
        EncoderError::TokioIO(other)
    }
}

impl From<toml::de::Error> for EncoderError {
    fn from(other: toml::de::Error) -> Self {
        EncoderError::TOML(other)
    }
}

impl From<crate::probe::VideoProbeError> for EncoderError {
    fn from(other: crate::probe::VideoProbeError) -> Self {
        EncoderError::VideoProbe(other)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_encoder_decode_toml() {
        let result: EncodingConfiguration = toml::from_str(
            r#"
            [vod.vp9]
            240p = { height = 240, bitrate = 157, audio_channels = 1, max_fps = 24 }
            360p = { height = 360, bitrate = 373, audio_channels = 2, max_fps = 24 }
            480p = { height = 480, bitrate = 727, audio_channels = 2, max_fps = 24 }
            720p = { height = 720, bitrate = 1468 , audio_channels = 2, max_fps = 60 }
            1080p = { height = 1080, bitrate = 2567, audio_channels = 2, max_fps = 60 }
            
            [vod.h264]
            240p = { height = 240, bitrate = 242, audio_channels = 1, max_fps = 24 }
            360p = { height = 360, bitrate = 525, audio_channels = 2, max_fps = 24 }
            480p = { height = 480, bitrate = 1155, audio_channels = 2, max_fps = 24 }
            720p = { height = 720, bitrate = 1378, audio_channels = 2, max_fps = 60 }
            1080p = { height = 1080, bitrate = 2309, audio_channels = 2, max_fps = 60 }
        "#,
        )
        .unwrap();

        println!("{:#?}", result);
    }
}
