//! lm-detector reference data ("fingerprint bank"): the built-in copy, host-state persistence and online updates.
//! A bank is stored packed as upstream's cache layout (u32 LE length of `bundle.json`, `bundle.json`,
//! `shared_detector.bin`) and identified by the SHA-256 of that packed form.
use crate::{
    engine::Engine,
    host::{self, fault},
    lmfpd,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use gateway_plugin_sdk::{PluginFault, call::host::HttpRequest, client::HostClient};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

const BUILTIN_BUNDLE: &[u8] = include_bytes!("../../vendor/lmfpd/data/bundle.json");
const BUILTIN_DETECTOR: &[u8] = include_bytes!("../../vendor/lmfpd/data/shared_detector.bin");
pub const SOURCE: &str = include_str!("../../vendor/lmfpd/SOURCE.json");
/// Current bank format; banks of the former ModelTrace engine have no format and are dropped on load.
pub const FORMAT: &str = "lmfpd-v1";
/// Upstream data sources, tried in order (the site, then its mainland-China npm mirror).
pub const DATA_URLS: &[&str] = &[
    "https://lm.ikale.io/data/",
    "https://cdn.jsdmirror.com/npm/lmfpd-data@latest/",
];
const MAX_RAW: usize = 16 * 1024 * 1024;
const MAX_BUNDLE: usize = 1024 * 1024;
const MAX_PART: usize = 8 * 1024 * 1024;
const CHUNK: usize = 180000;
/// Base64 chunks of one stored bank; the state namespace holds at most 16 MiB in total.
pub const MAX_CHUNKS: usize = 40;
#[derive(Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub label: String,
    pub family: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bank {
    pub hash: String,
    /// Data version: the SHA-256 of `shared_detector.bin`, as upstream names it.
    pub commit: String,
    pub built_at: String,
    pub models: Vec<Model>,
    pub chunks: usize,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub calibrated: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Update {
    pub last_attempt_at: Option<u64>,
    pub checked_at: Option<u64>,
    pub error: Option<String>,
    pub available: Option<Bank>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}
pub fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
pub fn engine_commit() -> String {
    serde_json::from_str::<Value>(SOURCE).unwrap()["commit"]
        .as_str()
        .unwrap()
        .into()
}
pub fn builtin_bytes() -> &'static [u8] {
    static PACKED: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    PACKED.get_or_init(|| lmfpd::pack(BUILTIN_BUNDLE, BUILTIN_DETECTOR))
}
pub fn builtin() -> Bank {
    static BANK: std::sync::OnceLock<Bank> = std::sync::OnceLock::new();
    BANK.get_or_init(|| inspect(builtin_bytes()).expect("built-in lm-detector data is valid"))
        .clone()
}
/// Decodes and self-checks packed data without touching the scoring cache.
fn inspect(packed: &[u8]) -> Result<Bank, PluginFault> {
    if packed.len() > MAX_RAW {
        return Err(fault("指纹库超过16MiB"));
    }
    let (_, detector_bytes) = lmfpd::unpack(packed).ok_or_else(|| fault("指纹库格式无效"))?;
    let detector =
        lmfpd::Detector::decode(packed).map_err(|e| fault(format!("指纹库解码失败：{e}")))?;
    let models = detector.models();
    if !(2..=256).contains(&models.len()) {
        return Err(fault("指纹库须包含2至256个模型"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for m in models {
        if m.id.is_empty() || m.id.len() > 128 || !ids.insert(m.id.as_str()) {
            return Err(fault("指纹库模型ID无效或重复"));
        }
        if m.display_name.len() > 128 || m.family.len() > 64 {
            return Err(fault("模型名称或家族无效"));
        }
    }
    // Exercise the ranker and calibration for one and three replies without sending a model request.
    let numbers: Vec<u32> = (0..310u32)
        .map(|i| {
            if i % 2 == 0 {
                (i * 137) % 177 + 1
            } else {
                178 + (i * 89) % 178
            }
        })
        .collect();
    for n in [1, 3] {
        let ranking = detector
            .rank(&vec![numbers.clone(); n])
            .filter(|r| r.len() == models.len() && r.iter().all(|x| x.is_finite()))
            .ok_or_else(|| fault("指纹库离线评分自检失败"))?;
        if let Some(p) = detector.calibrate(&ranking)
            && (p.iter().any(|x| !x.is_finite() || !(0.0..=1.0).contains(x))
                || (p.iter().sum::<f64>() - 1.0).abs() > 1e-8)
        {
            return Err(fault("指纹库置信度校准自检失败"));
        }
    }
    Ok(Bank {
        hash: hash(packed),
        commit: hash(detector_bytes),
        built_at: detector.built_at().into(),
        models: models
            .iter()
            .map(|m| Model {
                id: m.id.clone(),
                label: m.display_name.clone(),
                family: m.family.clone(),
            })
            .collect(),
        chunks: 0,
        format: FORMAT.into(),
        calibrated: detector.calibrated(),
    })
}
/// Validates packed data and warms the scoring cache with it.
pub fn validate(engine: &mut Engine, data: &[u8]) -> Result<Bank, PluginFault> {
    let bank = inspect(data)?;
    engine.load(&bank.hash, data)?;
    Ok(bank)
}
pub fn encode(data: &[u8]) -> Result<Vec<String>, PluginFault> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::best());
    enc.write_all(data).map_err(|_| fault("指纹库压缩失败"))?;
    let bytes = enc.finish().map_err(|_| fault("指纹库压缩失败"))?;
    let text = STANDARD.encode(bytes);
    let chunks: Vec<String> = text
        .as_bytes()
        .chunks(CHUNK)
        .map(|s| String::from_utf8(s.to_vec()).unwrap())
        .collect();
    if chunks.len() > MAX_CHUNKS {
        return Err(fault("指纹库压缩后超过存储预算"));
    }
    Ok(chunks)
}
pub async fn persist(host: &HostClient, bank: &Bank, chunks: &[String]) -> Result<(), PluginFault> {
    for (i, chunk) in chunks.iter().enumerate() {
        let key = format!("{}-{i}", bank.hash);
        let old = host::state_get(host, "banks", &key).await?;
        host::state_put(host, "banks", &key, chunk, old.map(|r| r.version)).await?;
    }
    Ok(())
}
pub async fn read(host: &HostClient, bank: &Bank) -> Result<Vec<u8>, PluginFault> {
    if bank.hash == builtin().hash {
        return Ok(builtin_bytes().to_vec());
    }
    if bank.chunks == 0 || bank.chunks > MAX_CHUNKS {
        return Err(fault("指纹库分块数量无效"));
    }
    let mut encoded = String::new();
    for i in 0..bank.chunks {
        let r = host::state_get(host, "banks", &format!("{}-{i}", bank.hash))
            .await?
            .ok_or_else(|| fault("指纹库数据缺失，请回退或恢复内置库"))?;
        encoded.push_str(
            r.value
                .as_str()
                .ok_or_else(|| fault("指纹库分块格式无效"))?,
        );
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| fault("指纹库分块损坏"))?;
    let mut data = vec![];
    GzDecoder::new(bytes.as_slice())
        .take(MAX_RAW as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| fault("指纹库解压失败"))?;
    if data.len() > MAX_RAW || hash(&data) != bank.hash {
        return Err(fault("指纹库摘要校验失败"));
    }
    Ok(data)
}
pub async fn remove(host: &HostClient, bank: &Bank) -> Result<(), PluginFault> {
    for i in 0..bank.chunks {
        host::state_delete(host, "banks", &format!("{}-{i}", bank.hash)).await?;
    }
    Ok(())
}
async fn fetch(host: &HostClient, url: &str, limit: usize) -> Result<Vec<u8>, PluginFault> {
    let r = host
        .http(
            HttpRequest {
                method: "GET".into(),
                url: url.into(),
                headers: vec![(
                    "User-Agent".into(),
                    concat!("modeltrace-health/", env!("CARGO_PKG_VERSION"))
                        .as_bytes()
                        .to_vec(),
                )],
            },
            vec![],
        )
        .await?;
    if r.status != 200 {
        let status = r.status;
        let _ = r.body.collect(1024).await;
        return Err(fault(format!("数据源响应HTTP {status}")));
    }
    r.body.collect(limit).await
}
/// Data sources: the configured mirror prefix alone (prefix + upstream URL), or the upstream defaults in order.
fn sources(mirror: &str) -> Vec<String> {
    if mirror.is_empty() {
        DATA_URLS.iter().map(|u| u.to_string()).collect()
    } else {
        vec![format!("{}/{}", mirror.trim_end_matches('/'), DATA_URLS[0])]
    }
}
fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn decompress(compressed: &[u8]) -> Result<Vec<u8>, PluginFault> {
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(compressed)
        .map_err(|_| fault("检测器数据解压失败"))?;
    let mut content = Vec::new();
    (&mut decoder)
        .take(MAX_RAW as u64 + 1)
        .read_to_end(&mut content)
        .map_err(|_| fault("检测器数据解压失败"))?;
    if content.len() > MAX_RAW {
        return Err(fault("检测器数据超过16MiB"));
    }
    Ok(content)
}
async fn download_from(
    host: &HostClient,
    source: &str,
    expected: Option<&str>,
) -> Result<Vec<u8>, PluginFault> {
    let bundle_bytes = fetch(host, &format!("{source}bundle.json"), MAX_BUNDLE).await?;
    let bundle: Value =
        serde_json::from_slice(&bundle_bytes).map_err(|_| fault("bundle.json 格式无效"))?;
    if bundle["schema"] != lmfpd::DATA_SCHEMA || bundle["detector_schema"] != lmfpd::DETECTOR_SCHEMA
    {
        return Err(fault("上游数据格式已变化，请升级插件"));
    }
    let file = &bundle["files"]["shared_detector.bin"];
    let version = file["sha256"]
        .as_str()
        .filter(|v| is_sha256(v))
        .ok_or_else(|| fault("bundle.json 未提供检测器摘要"))?;
    if let Some(expected) = expected
        && expected != version
    {
        return Err(fault("数据源版本已变化，请重新检查更新"));
    }
    let parts = file["parts"]
        .as_array()
        .filter(|p| !p.is_empty() && p.len() <= 16)
        .ok_or_else(|| fault("bundle.json 检测器分片无效"))?;
    let mut compressed = Vec::new();
    for part in parts {
        let part = part
            .as_str()
            .filter(|p| {
                !p.is_empty()
                    && p.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
            })
            .ok_or_else(|| fault("bundle.json 检测器分片无效"))?;
        compressed.extend(fetch(host, &format!("{source}chunks/{part}"), MAX_PART).await?);
        if compressed.len() > MAX_RAW {
            return Err(fault("检测器数据超过16MiB"));
        }
    }
    let detector = decompress(&compressed)?;
    if hash(&detector) != version {
        return Err(fault("检测器摘要校验失败"));
    }
    Ok(lmfpd::pack(&bundle_bytes, &detector))
}
/// Downloads packed data from the first working source; with `expected`, only that detector version is accepted.
pub async fn download(
    host: &HostClient,
    mirror: &str,
    expected: Option<&str>,
) -> Result<Vec<u8>, PluginFault> {
    let mut failures = vec![];
    for source in sources(mirror) {
        match download_from(host, &source, expected).await {
            Ok(data) => return Ok(data),
            Err(e) => failures.push(format!("{source}（{}）", e.message)),
        }
    }
    Err(fault(format!("无法下载指纹库：{}", failures.join("；"))))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compressed_roundtrip_and_builtin_identity() {
        let data = builtin_bytes();
        let c = encode(data).unwrap();
        let b = STANDARD.decode(c.join("")).unwrap();
        let mut out = vec![];
        GzDecoder::new(b.as_slice()).read_to_end(&mut out).unwrap();
        assert_eq!(out, data);
        assert_eq!(builtin().hash, hash(&out));
        assert!(c.iter().all(|c| c.len() <= CHUNK));
        assert!(builtin().calibrated);
        assert_eq!(builtin().format, FORMAT);
    }
}
