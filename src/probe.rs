use std::process::Command;
use std::path::{Path, PathBuf};

use crate::monitor::NginxMeta;

#[derive(Debug)]
pub struct VideoProbe {
    file: PathBuf,
}

impl VideoProbe {
    pub fn new(file: &Path) -> Result<Self, VideoProbeError> {
        if !file.exists() {
            return Err(VideoProbeError::FileNotFound);
        }

        Ok(VideoProbe { file: file.to_path_buf() })
    }

    pub fn probe(self) -> Result<Vec<NginxMeta>, VideoProbeError> {
        let output = Command::new("ffprobe")
            .args(&["-v", "quiet"])
            .args(&["-print_format", "json"])
            .args(&["-print_format", "json"])
            .args(&["-show_format"])
            .args(&["-show_streams"])
            .arg(self.file.to_str().unwrap())
            .output()?;

        if !output.status.success() {
            return Err(VideoProbeError::CommandFailed);
        }

        let val: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let format = NginxMeta::Format {
            duration: val["format"]["duration"].as_str().ok_or(VideoProbeError::MalformedOutput)?.parse().map_err(|_| VideoProbeError::MalformedOutput)?,
            size: val["format"]["size"].as_str().ok_or(VideoProbeError::MalformedOutput)?.parse().map_err(|_| VideoProbeError::MalformedOutput)?,
        };

        val["streams"]
            .as_array()
            .ok_or(VideoProbeError::MalformedOutput)?
            .into_iter()
            .filter(|s| s["codec_type"].as_str() == Some("video") || s["codec_type"].as_str() == Some("audio"))
            .map(|s| -> Result<NginxMeta, VideoProbeError> {
                match s["codec_type"].as_str() {
                    Some("video") => {
                        let frame_rate_str = s["r_frame_rate"].as_str().ok_or(VideoProbeError::MalformedOutput)?;
                        let frame_rate_parts = frame_rate_str.split("/").collect::<Vec<&str>>();
                        if frame_rate_parts.len() != 2 {
                            return Err(VideoProbeError::MalformedOutput);
                        }
                        let frame_rate_parts = frame_rate_parts.into_iter().map(|s| s.parse::<u32>()).collect::<Result<Vec<_>, _>>().map_err(|_| VideoProbeError::MalformedOutput)?;
                        let frame_rate = frame_rate_parts[0] as f32 / frame_rate_parts[1] as f32;

                        Ok(NginxMeta::Video {
                            width: s["width"].as_u64().ok_or(VideoProbeError::MalformedOutput)? as usize,
                            height: s["height"].as_u64().ok_or(VideoProbeError::MalformedOutput)? as usize,
                            frame_rate,
                            codec: s["codec_name"].as_str().ok_or(VideoProbeError::MalformedOutput)?.into(),
                            profile: s["profile"].as_str().ok_or(VideoProbeError::MalformedOutput)?.into(),
                        })
                    },
                    Some("audio") => {
                        Ok(NginxMeta::Audio {
                            channels: s["channels"].as_u64().ok_or(VideoProbeError::MalformedOutput)? as usize,
                            sample_rate: s["sample_rate"].as_str().ok_or(VideoProbeError::MalformedOutput)?.parse().map_err(|_| VideoProbeError::MalformedOutput)?,
                            codec: s["codec_name"].as_str().ok_or(VideoProbeError::MalformedOutput)?.into(),
                            profile: s["profile"].as_str().ok_or(VideoProbeError::MalformedOutput)?.into(),
                        })
                    },
                    _ => unimplemented!(),
                }
            })
            .chain(vec![Ok(format)].into_iter())
            .collect()
    }
}

#[derive(Debug)]
pub enum VideoProbeError {
    FileNotFound,
    CommandFailed,
    MalformedOutput,

    IO(std::io::Error),
    JSON(serde_json::Error),
}

impl From<std::io::Error> for VideoProbeError {
    fn from(other: std::io::Error) -> Self {
        VideoProbeError::IO(other)
    }
}

impl From<serde_json::Error> for VideoProbeError {
    fn from(other: serde_json::Error) -> Self {
        VideoProbeError::JSON(other)
    }
}
