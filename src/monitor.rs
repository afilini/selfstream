use std::sync::RwLock;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_xml_rs::from_str;

use log::trace;

#[derive(Debug)]
pub struct NginxMonitor {
    endpoint: String,
    last_result: RwLock<Option<(SystemTime, NginxStat)>>,
}

impl NginxMonitor {
    pub fn new(endpoint: &str) -> Self {
        NginxMonitor {
            endpoint: endpoint.to_string(),
            last_result: RwLock::new(None),
        }
    }

    pub async fn update(&self) -> Result<NginxStat, MonitorError> {
        trace!("Updating nginx monitor data");

        let body = reqwest::get(&self.endpoint).await?.text().await?;
        let parsed: NginxStat = from_str(&body)?;

        *self.last_result.write().unwrap() = Some((SystemTime::now(), parsed.clone()));

        Ok(parsed)
    }

    pub async fn get_newer_than(&self, duration: Duration) -> Result<NginxStat, MonitorError> {
        if let Some((time, data)) = self.last_result.read().unwrap().as_ref() {
            if SystemTime::now().duration_since(*time).unwrap() <= duration {
                return Ok(data.clone());
            }
        }

        self.update().await
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename = "rtmp")]
pub struct NginxStat {
    pub nginx_version: String,
    pub nginx_rtmp_version: String,
    pub uptime: usize,
    pub server: NginxApplicationsVec,
}

impl NginxStat {
    pub fn get_application(&self, name: &str) -> Option<&NginxApplication> {
        self.server.applications.iter().find(|a| a.name == name)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NginxApplicationsVec {
    #[serde(rename = "application")]
    pub applications: Vec<NginxApplication>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename = "application")]
pub struct NginxApplication {
    pub name: String,
    pub live: NginxStreamsVec,
}

impl NginxApplication {
    pub fn get_stream(&self, name: &str) -> Option<&NginxStream> {
        self.live
            .streams
            .as_ref()
            .and_then(|arr| arr.iter().find(|a| a.name == name))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NginxStreamsVec {
    #[serde(rename = "stream")]
    pub streams: Option<Vec<NginxStream>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename = "stream")]
pub struct NginxStream {
    pub name: String,
    pub time: usize,
    pub bw_in: usize,
    pub bytes_in: usize,
    pub bw_out: usize,
    pub bytes_out: usize,
    pub client: Vec<NginxClient>,
    pub meta: Option<NginxMetasVec>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename = "client")]
pub struct NginxClient {
    pub id: usize,
    pub address: String,
    pub flashver: String,
    pub dropped: usize,
    pub timestamp: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NginxMetasVec {
    #[serde(rename = "$value", default)]
    pub metas: Vec<NginxMeta>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum NginxMeta {
    #[serde(rename = "video")]
    Video {
        width: usize,
        height: usize,
        frame_rate: f32,
        codec: String,
        profile: String,
    },
    #[serde(rename = "audio")]
    Audio {
        codec: String,
        profile: String,
        channels: usize,
        sample_rate: usize,
    },
    Format {
        duration: f32,
        size: usize,
    },
}

#[derive(Debug)]
pub enum MonitorError {
    Reqwest(reqwest::Error),
    XML(serde_xml_rs::Error),
}

impl From<reqwest::Error> for MonitorError {
    fn from(other: reqwest::Error) -> Self {
        MonitorError::Reqwest(other)
    }
}

impl From<serde_xml_rs::Error> for MonitorError {
    fn from(other: serde_xml_rs::Error) -> Self {
        MonitorError::XML(other)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_xml_rs::from_str;

    #[test]
    fn test_control() {
        let s = r#"<rtmp>
<nginx_version>1.18.0</nginx_version>
<nginx_rtmp_version>1.1.4</nginx_rtmp_version>
<compiler>gcc 9.3.1 20200408 (Red Hat 9.3.1-2) (GCC) </compiler>
<built>Apr 22 2020 14:25:42</built>
<pid>11832</pid>
<uptime>1489</uptime>
<naccepted>95</naccepted>
<bw_in>0</bw_in>
<bytes_in>100472939</bytes_in>
<bw_out>0</bw_out>
<bytes_out>43208384</bytes_out>
<server>
<application>
<name>hls</name>
<live>
<stream>
<name>movie_480</name>
<time>1485023</time><bw_in>0</bw_in>
<bytes_in>66600586</bytes_in>
<bw_out>0</bw_out>
<bytes_out>0</bytes_out>
<bw_audio>0</bw_audio>
<bw_video>0</bw_video>
<client><id>4</id><address>127.0.0.1</address><time>1485187</time><flashver>FMLE/3.0 (compatible; Lavf58.20</flashver><dropped>0</dropped><avsync>-2</avsync><timestamp>461940</timestamp><publishing/><active/></client>
<meta><video><width>1146</width><height>480</height><frame_rate>24</frame_rate><codec>H264</codec><profile>Baseline</profile><compat>192</compat><level>3.1</level></video><audio><codec>AAC</codec><profile>LC</profile><channels>2</channels><sample_rate>44100</sample_rate></audio></meta>
<nclients>1</nclients>
<publishing/>
<active/>
</stream>
<nclients>1</nclients>
</live>
</application>
<application>
<name>src</name>
<live>
<stream>
<name>movie</name>
<time>1487111</time><bw_in>0</bw_in>
<bytes_in>33293265</bytes_in>
<bw_out>0</bw_out>
<bytes_out>42768131</bytes_out>
<bw_audio>0</bw_audio>
<bw_video>0</bw_video>
<client><id>97</id><address>127.0.0.1</address><time>966762</time><flashver>LNX 9,0,124,2</flashver><dropped>0</dropped><avsync>74</avsync><timestamp>464042</timestamp></client>
<client><id>3</id><address>127.0.0.1</address><time>1487064</time><flashver>LNX 9,0,124,2</flashver><dropped>0</dropped><avsync>74</avsync><timestamp>464042</timestamp></client>
<nclients>2</nclients>
</stream>
<nclients>2</nclients>
</live>
</application>
</server>
</rtmp>"#;

        let n: NginxStat = from_str(s).unwrap();
        println!("{:#?}", n);
    }
}
