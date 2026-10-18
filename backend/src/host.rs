use crate::domain::{Account, Group, Identity};
use gateway_plugin_sdk::{
    ErrorCode, PluginFault,
    call::{
        host::*,
        middleware::{MiddlewareHeader, http::Version},
        model::WirePayload,
    },
    client::{HostClient, HttpBody, HttpFrame, HttpRequest as DispatchRequest},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn fault(message: impl Into<String>) -> PluginFault {
    PluginFault::new(ErrorCode::Rejected, message)
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub async fn meta<I: Serialize, O: DeserializeOwned>(
    host: &HostClient,
    method: &str,
    input: I,
) -> Result<O, PluginFault> {
    let r = host
        .call(
            method,
            serde_json::to_value(input).map_err(|_| fault("请求编码失败"))?,
            vec![],
        )
        .await
        .map_err(|e| e.into_plugin_fault())?;
    if !r.payload.is_empty() {
        return Err(fault("宿主元数据格式无效"));
    }
    serde_json::from_value(r.result).map_err(|_| fault("宿主返回格式无效"))
}
pub async fn runtime(host: &HostClient, id: &str) -> Result<AuthRuntimeAccount, PluginFault> {
    let r = host
        .call(
            "host.auth.get_runtime",
            json!({}),
            serde_json::to_vec(&json!({"account_id":id})).unwrap(),
        )
        .await
        .map_err(|e| e.into_plugin_fault())?;
    serde_json::from_slice(&r.payload).map_err(|_| fault("账户运行状态读取失败"))
}
pub fn identity(a: &AuthRuntimeAccount) -> Identity {
    Identity {
        authentication_kind: a.authentication_kind.clone(),
        upstream_user_id: a.upstream_user_id.clone(),
        upstream_account_id: a.upstream_account_id.clone(),
    }
}
pub async fn state_get(
    host: &HostClient,
    ns: &str,
    key: &str,
) -> Result<Option<StateRecord>, PluginFault> {
    let r: StateGetResult = meta(
        host,
        "host.state.get",
        StateGetRequest {
            namespace: ns.into(),
            key: key.into(),
        },
    )
    .await?;
    Ok(r.record)
}
pub async fn state_put<T: Serialize>(
    host: &HostClient,
    ns: &str,
    key: &str,
    value: &T,
    version: Option<u64>,
) -> Result<u64, PluginFault> {
    let value = serde_json::to_value(value).map_err(|_| fault("状态编码失败"))?;
    if serde_json::to_vec(&value).unwrap().len() > 250000 {
        return Err(fault("单条状态超过存储上限"));
    }
    let r: StatePutResult = meta(
        host,
        "host.state.put",
        StatePutRequest {
            namespace: ns.into(),
            key: key.into(),
            value,
            expected_version: version,
        },
    )
    .await?;
    Ok(r.version)
}
pub async fn state_delete(host: &HostClient, ns: &str, key: &str) -> Result<(), PluginFault> {
    if let Some(r) = state_get(host, ns, key).await? {
        let _: StateDeleteResult = meta(
            host,
            "host.state.delete",
            StateDeleteRequest {
                namespace: ns.into(),
                key: key.into(),
                expected_version: r.version,
            },
        )
        .await?;
    }
    Ok(())
}
pub async fn admin(
    host: &HostClient,
    method: &str,
    uri: &str,
    data: Option<Value>,
) -> Result<Value, PluginFault> {
    let body = data
        .map(|v| HttpBody::from_bytes(serde_json::to_vec(&v).unwrap()))
        .unwrap_or_else(HttpBody::empty);
    let mut r = host
        .dispatch_http(DispatchRequest {
            settings: Value::Null,
            method: method.into(),
            uri: uri.into(),
            version: Version::Http11,
            headers: vec![MiddlewareHeader {
                name: "content-type".into(),
                value: b"application/json".to_vec(),
            }],
            timeout_ms: Some(5000),
            body,
        })
        .await?;
    let mut bytes = vec![];
    while let Some(frame) = r.body.read().await? {
        if let HttpFrame::Data(c) = frame {
            if bytes.len() + c.len() > 2 * 1024 * 1024 {
                let _ = r.body.close().await;
                return Err(fault("账户列表响应过大"));
            }
            bytes.extend(c);
        }
    }
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| fault("宿主管理接口格式无效"))?;
    if !(200..300).contains(&r.status) || v["code"] != 200 {
        return Err(fault(format!("宿主管理接口失败（HTTP {}）", r.status)));
    }
    Ok(v["data"].clone())
}
pub async fn accounts(host: &HostClient) -> Result<Vec<Account>, PluginFault> {
    let mut result = vec![];
    let mut seen = BTreeSet::new();
    for page in 1..=1000 {
        let data = admin(
            host,
            "GET",
            &format!("/api/admin/accounts?page={page}&pageSize=100"),
            None,
        )
        .await?;
        let items = data["items"]
            .as_array()
            .ok_or_else(|| fault("账户列表缺少items"))?;
        for a in items {
            let id = a["id"]
                .as_str()
                .ok_or_else(|| fault("账户ID无效"))?
                .to_string();
            if !seen.insert(id.clone()) {
                return Err(fault("宿主账户分页重复"));
            }
            result.push(Account {
                id,
                name: a["name"].as_str().unwrap_or_default().into(),
                email: a["email"].as_str().map(str::to_string),
                provider: a["provider"].as_str().unwrap_or_default().into(),
                enabled: a["enabled"].as_bool().unwrap_or(false),
                status: a["status"].as_str().unwrap_or("unknown").into(),
                plan_type: a["planType"].as_str().map(|s| s.trim().to_lowercase()),
                groups: a["groups"]
                    .as_array()
                    .map(|g| {
                        g.iter()
                            .filter_map(|g| {
                                Some(Group {
                                    id: g["id"].as_str()?.into(),
                                    name: g["name"].as_str()?.into(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                supported: a["provider"] == "openai",
            });
        }
        if items.len() < 100
            || data["total"]
                .as_u64()
                .is_some_and(|total| result.len() as u64 >= total)
        {
            return Ok(result);
        }
    }
    Err(fault("账户数量超过100000"))
}
pub async fn groups(host: &HostClient) -> Result<Vec<Group>, PluginFault> {
    let mut result = vec![];
    let mut seen = BTreeSet::new();
    for page in 1..=1000 {
        let data = admin(
            host,
            "GET",
            &format!("/api/admin/account-groups/options?page={page}&pageSize=200"),
            None,
        )
        .await?;
        let items = data["items"]
            .as_array()
            .ok_or_else(|| fault("分组列表缺少items"))?;
        for item in items {
            let group: Group =
                serde_json::from_value(item.clone()).map_err(|_| fault("分组信息无效"))?;
            if !seen.insert(group.id.clone()) {
                return Err(fault("宿主分组分页重复"));
            }
            result.push(group);
        }
        if items.len() < 200
            || data["page"]["totalPages"]
                .as_u64()
                .is_some_and(|total| page >= total)
        {
            return Ok(result);
        }
    }
    Err(fault("分组数量超过200000"))
}
pub async fn account_groups(
    host: &HostClient,
    account_id: &str,
) -> Result<Vec<String>, PluginFault> {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("accountId", account_id)
        .finish();
    let data = admin(
        host,
        "GET",
        &format!("/api/admin/accounts/detail?{query}"),
        None,
    )
    .await?;
    if data["account"]["id"].as_str() != Some(account_id) {
        return Err(fault("宿主返回了不同账户，移组已终止"));
    }
    data["account"]["groups"]
        .as_array()
        .ok_or_else(|| fault("账户分组信息缺失"))?
        .iter()
        .map(|g| {
            g["id"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| fault("账户分组ID无效"))
        })
        .collect()
}
pub async fn keys(host: &HostClient) -> Result<Vec<ClientKey>, PluginFault> {
    let mut cursor = None;
    let mut keys = vec![];
    let mut seen = BTreeSet::new();
    loop {
        let p = host
            .list_keys(KeyListRequest { cursor, limit: 100 })
            .await?;
        keys.extend(p.keys.into_iter().filter(|k| k.enabled));
        cursor = p.next_cursor;
        let Some(c) = &cursor else { return Ok(keys) };
        if !seen.insert(c.clone()) || seen.len() > 100 {
            return Err(fault("Key分页无效"));
        }
    }
}

pub struct ExecutionError {
    pub request_id: Option<String>,
    pub message: String,
    pub permanent: bool,
    pub diagnostics: crate::domain::ProbeDiagnostics,
}

/// Pin every probe to its submitted account. The host resolves credentials and
/// the account's configured outbound proxy from this account ID; omitting it
/// would allow normal account selection to choose a different route.
fn model_execute_request(account_id: &str, cfg: &crate::domain::Config) -> ModelExecuteRequest {
    ModelExecuteRequest {
        client_key_id: Some(cfg.key_id.clone()),
        model: cfg.request_model.clone(),
        protocol: "openai".into(),
        operation: ModelOperation::Generate,
        provider: Some("openai".into()),
        account_id: Some(account_id.into()),
        previous_response_id: None,
    }
}

pub async fn execute(
    host: &HostClient,
    job: &crate::domain::Job,
    prompt: &str,
    parent_deadline: Option<tokio::time::Instant>,
    progress: std::sync::Arc<crate::progress::Progress>,
) -> Result<(String, String), ExecutionError> {
    let cfg = &job.config;
    let mut body = json!({"model":cfg.request_model,"input":[{"role":"user","content":[{"type":"input_text","text":prompt}]}],"stream":true,"store":false,"max_output_tokens":cfg.max_output_tokens});
    if let Some(t) = cfg.temperature {
        body["temperature"] = json!(t);
    }
    if !cfg.reasoning_effort.is_empty() {
        body["reasoning"] = json!({"effort":cfg.reasoning_effort});
    }
    let mut stream = None;
    let mut request_id = None;
    let started = tokio::time::Instant::now();
    let mut received_bytes = 0;
    let configured_deadline = (cfg.timeout_seconds != 0)
        .then(|| started.checked_add(std::time::Duration::from_secs(cfg.timeout_seconds)))
        .flatten();
    let host_limited = parent_deadline
        .is_some_and(|parent| configured_deadline.is_none_or(|configured| parent <= configured));
    let deadline = match (configured_deadline, parent_deadline) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let budget_ms = deadline.map(|deadline| {
        deadline
            .saturating_duration_since(started)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    });
    let request = async {
        let r = host
            .call(
                "host.model.execute_stream",
                serde_json::to_value(model_execute_request(&job.account_id, cfg)).unwrap(),
                serde_json::to_vec(&body).unwrap(),
            )
            .await
            .map_err(|e| e.into_plugin_fault())?;
        let result: ModelStreamResult =
            serde_json::from_value(r.result).map_err(|_| fault("宿主模型流格式无效"))?;
        request_id = Some(result.request_id);
        progress.phase(&job.id, "waiting_response", json!({"requestId":request_id}));
        stream = Some(result.stream.clone());
        let mut sse = String::new();
        let mut last_update = tokio::time::Instant::now() - std::time::Duration::from_secs(1);
        loop {
            let r = host
                .call(
                    "host.model.stream_read",
                    json!({"stream":result.stream,"maximum_bytes":262144}),
                    vec![],
                )
                .await
                .map_err(|e| e.into_plugin_fault())?;
            let state: ModelStreamReadResult =
                serde_json::from_value(r.result).map_err(|_| fault("流状态格式无效"))?;
            let batch = if r.payload.is_empty() && state.events == 0 {
                ModelEventBatch::default()
            } else {
                ModelEventBatch::decode(&r.payload).map_err(|_| fault("流事件格式无效"))?
            };
            if batch.events.len() != state.events as usize {
                return Err(fault("流事件数量不一致"));
            }
            for e in batch.events {
                if let Some(w) = e.wire {
                    if w.protocol != "openai" {
                        return Err(fault("模型协议无效"));
                    }
                    match w.payload {
                        WirePayload::Json { data, .. } => {
                            sse.push_str("data: ");
                            sse.push_str(&data.to_string());
                            sse.push_str("\n\n");
                        }
                        WirePayload::RawSse { frame } => sse.push_str(
                            std::str::from_utf8(&frame).map_err(|_| fault("流编码无效"))?,
                        ),
                        _ => return Err(fault("模型未返回Responses事件")),
                    }
                    if sse.len() > 1024 * 1024 {
                        return Err(fault("回答超过1MiB"));
                    }
                    received_bytes = sse.len();
                }
            }
            if last_update.elapsed() >= std::time::Duration::from_millis(250) || state.end {
                progress.phase(
                    &job.id,
                    "receiving",
                    json!({"requestId":request_id,"receivedBytes":sse.len()}),
                );
                last_update = tokio::time::Instant::now();
            }
            if state.end {
                stream = None;
                break;
            }
        }
        parse_sse(&sse)
    };
    // Zero disables the plugin deadline. Huge durations that cannot fit the
    // monotonic clock also have no reachable deadline; never overflow Instant.
    let bounded = async {
        match deadline {
            Some(deadline) => tokio::time::timeout_at(deadline, request).await,
            None => Ok(request.await),
        }
    };
    let result = tokio::select! {
        biased;
        ()=progress.cancelled(&job.id)=>Ok(Err(fault("管理员取消检测"))),
        result=bounded=>result,
    };
    if let Some(s) = stream {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            host.call("host.model.stream_close", json!({"stream":s}), vec![]),
        )
        .await;
    }
    match result {
        Ok(Ok(text)) => Ok((request_id.unwrap_or_default(), text)),
        other => {
            let timed_out = other.is_err();
            let (message, typed_permanent) = match other {
                Ok(Err(e)) => {
                    let permanent = matches!(
                        e.code,
                        ErrorCode::InvalidInput | ErrorCode::PermissionDenied
                    ) || matches!(e.http_status, Some(400 | 401 | 403 | 404));
                    (e.message, permanent)
                }
                _ => ("模型请求超时".into(), false),
            };
            let lower = message.to_lowercase();
            let permanent = typed_permanent
                || [
                    "401",
                    "403",
                    "auth",
                    "credential",
                    "permission",
                    "unauthorized",
                    "invalid",
                    "unsupported",
                    "无效",
                    "不存在",
                    "停用",
                    "参数",
                    "不支持",
                ]
                .iter()
                .any(|s| lower.contains(s));
            // Host faults may contain upstream text; return categories rather than credentials or headers.
            let kind = if timed_out && host_limited {
                "host_budget_timeout"
            } else if timed_out {
                "request_timeout"
            } else if permanent {
                "request_rejected"
            } else {
                "upstream_failure"
            };
            Err(ExecutionError {
                request_id,
                message: if timed_out && host_limited {
                    format!(
                        "宿主调用预算超时（本次请求窗口约{:.1}秒，已接收{:.1}KB流数据）",
                        budget_ms.unwrap_or_default() as f64 / 1000.0,
                        received_bytes as f64 / 1024.0
                    )
                } else if permanent {
                    "账户认证、Key权限或模型参数被拒绝，请检查宿主日志".into()
                } else if lower.contains("429") || lower.contains("rate_limit") {
                    "上游限流".into()
                } else if lower.contains("超时") {
                    "模型请求超时".into()
                } else {
                    "模型请求失败或未完整结束，请检查宿主请求记录".into()
                },
                permanent,
                diagnostics: crate::domain::ProbeDiagnostics {
                    kind: kind.into(),
                    elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                    received_bytes,
                    budget_ms,
                },
            })
        }
    }
}
pub fn parse_sse(sse: &str) -> Result<String, PluginFault> {
    let normalized = sse.replace("\r\n", "\n");
    let mut delta = String::new();
    let mut completed = None;
    for frame in normalized.split("\n\n") {
        let data = frame
            .lines()
            .filter_map(|l| l.strip_prefix("data:").map(str::trim_start))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let v: Value = serde_json::from_str(&data).map_err(|_| fault("模型流JSON无效"))?;
        match v["type"].as_str().unwrap_or("") {
            "response.output_text.delta" => delta.push_str(v["delta"].as_str().unwrap_or("")),
            "response.completed" => {
                let text = v["response"]["output"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|i| i["content"].as_array())
                            .flatten()
                            .filter(|c| c["type"] == "output_text")
                            .filter_map(|c| c["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                completed = Some(text);
            }
            "response.failed" | "response.incomplete" | "error" => {
                return Err(fault(format!(
                    "模型未完整结束 {}",
                    v["error"]["code"]
                        .as_str()
                        .or(v["response"]["error"]["code"].as_str())
                        .unwrap_or("upstream_error")
                )));
            }
            _ => {}
        }
    }
    let text = completed.ok_or_else(|| fault("缺少模型完成事件"))?;
    let text = if text.is_empty() { delta } else { text };
    if text.is_empty() {
        return Err(fault("模型回答为空"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_request_pins_account_for_proxy_routing() {
        let request =
            model_execute_request("account-with-proxy", &crate::domain::Config::default());
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["account_id"], "account-with-proxy");
        assert_eq!(value["provider"], "openai");
    }

    #[test]
    fn only_complete_text_is_accepted() {
        let s = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"123\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"content\":[{\"type\":\"output_text\",\"text\":\"123\"}]}]}}\n\n";
        assert_eq!(parse_sse(s).unwrap(), "123");
        assert!(
            parse_sse("data: {\"type\":\"response.output_text.delta\",\"delta\":\"123\"}\n\n")
                .is_err()
        );
        assert!(parse_sse("data: {\"type\":\"response.incomplete\"}\n\n").is_err());
    }
}
