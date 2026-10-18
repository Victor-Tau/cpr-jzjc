use crate::{
    engine::Engine,
    host::{self, fault},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use gateway_plugin_sdk::{PluginFault, call::host::HttpRequest, client::HostClient};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

pub const BUILTIN: &[u8] = include_bytes!("../../vendor/modeltrace/data/unified_bank.json");
pub const SOURCE: &str = include_str!("../../vendor/modeltrace/SOURCE.json");
const MAX_RAW: usize = 20 * 1024 * 1024;
const CHUNK: usize = 180000;
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
    pub commit: String,
    pub built_at: String,
    pub models: Vec<Model>,
    pub chunks: usize,
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
fn builtin_metadata() -> Bank {
    let b: Value = serde_json::from_slice(BUILTIN).unwrap();
    Bank {
        hash: hash(BUILTIN),
        commit: engine_commit(),
        built_at: b["built_at"].as_str().unwrap_or("").into(),
        models: b["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| Model {
                id: m["id"].as_str().unwrap().into(),
                label: m["display_name"].as_str().unwrap().into(),
                family: m["family"].as_str().unwrap().into(),
            })
            .collect(),
        chunks: 0,
    }
}
pub fn builtin() -> Bank {
    static BANK: std::sync::OnceLock<Bank> = std::sync::OnceLock::new();
    BANK.get_or_init(builtin_metadata).clone()
}
pub async fn validate(
    engine: &mut Engine,
    data: &[u8],
    commit: String,
) -> Result<Bank, PluginFault> {
    let digest = hash(data);
    let path = engine.write_bank(&digest, data).await?;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Info {
        built_at: String,
        models: Vec<Model>,
    }
    let info: Info = engine.call(json!({"op":"validate","path":path})).await?;
    Ok(Bank {
        hash: digest,
        commit,
        built_at: info.built_at,
        models: info.models,
        chunks: 0,
    })
}
pub fn encode(data: &[u8]) -> Result<Vec<String>, PluginFault> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(data).map_err(|_| fault("指纹库压缩失败"))?;
    let bytes = enc.finish().map_err(|_| fault("指纹库压缩失败"))?;
    if bytes.len() > 3 * 1024 * 1024 {
        return Err(fault("指纹库压缩后超过3MiB存储预算"));
    }
    let text = STANDARD.encode(bytes);
    Ok(text
        .as_bytes()
        .chunks(CHUNK)
        .map(|s| String::from_utf8(s.to_vec()).unwrap())
        .collect())
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
        return Ok(BUILTIN.to_vec());
    }
    if bank.chunks == 0 || bank.chunks > 24 {
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
async fn fetch(
    host: &HostClient,
    url: &str,
    mirror: &str,
    limit: usize,
) -> Result<Vec<u8>, PluginFault> {
    let url = if mirror.is_empty() {
        url.into()
    } else {
        format!("{}/{url}", mirror.trim_end_matches('/'))
    };
    let r = host
        .http(
            HttpRequest {
                method: "GET".into(),
                url,
                headers: vec![
                    ("User-Agent".into(), "modeltrace-health/1.2.0".into()),
                    ("Accept".into(), "application/vnd.github+json".into()),
                ],
            },
            vec![],
        )
        .await?;
    if r.status != 200 {
        let status = r.status;
        let _ = r.body.collect(1024).await;
        return Err(fault(format!("GitHub或镜像响应HTTP {status}，未确认更新")));
    }
    r.body.collect(limit).await
}
pub async fn latest(host: &HostClient, mirror: &str) -> Result<String, PluginFault> {
    let repo: Value = serde_json::from_slice(
        &fetch(
            host,
            "https://api.github.com/repos/xqy2006/ModelTrace",
            mirror,
            256 * 1024,
        )
        .await?,
    )
    .map_err(|_| fault("GitHub仓库信息无效"))?;
    let branch = repo["default_branch"]
        .as_str()
        .ok_or_else(|| fault("GitHub未返回默认分支"))?;
    let branch: String = url::form_urlencoded::byte_serialize(branch.as_bytes()).collect();
    let commit: Value = serde_json::from_slice(
        &fetch(
            host,
            &format!("https://api.github.com/repos/xqy2006/ModelTrace/commits/{branch}"),
            mirror,
            512 * 1024,
        )
        .await?,
    )
    .map_err(|_| fault("GitHub提交信息无效"))?;
    let sha = commit["sha"]
        .as_str()
        .ok_or_else(|| fault("GitHub未返回提交SHA"))?;
    if sha.len() != 40 || !sha.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(fault("GitHub提交SHA无效"));
    }
    Ok(sha.into())
}
pub async fn download(
    host: &HostClient,
    mirror: &str,
    commit: &str,
) -> Result<Vec<u8>, PluginFault> {
    if commit.len() != 40 || !commit.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(fault("提交SHA无效"));
    }
    fetch(
        host,
        &format!(
            "https://raw.githubusercontent.com/xqy2006/ModelTrace/{commit}/data/unified_bank.json"
        ),
        mirror,
        MAX_RAW,
    )
    .await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compressed_roundtrip_and_builtin_identity() {
        let c = encode(BUILTIN).unwrap();
        let b = STANDARD.decode(c.join("")).unwrap();
        let mut out = vec![];
        GzDecoder::new(b.as_slice()).read_to_end(&mut out).unwrap();
        assert_eq!(out, BUILTIN);
        assert_eq!(builtin().hash, hash(&out));
        assert!(c.iter().all(|c| c.len() <= CHUNK));
    }
}
