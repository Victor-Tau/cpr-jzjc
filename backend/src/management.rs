use crate::host::fault;
use gateway_plugin_sdk::{PluginFault, call::management::ManagementResponse, client::TypedReply};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

// CPR 管理正文上限为 1 MiB；分页预留元数据空间，按 UTF-8 编码后的字节数计量。
const PAGE_BYTES: usize = 768 * 1024;
const MAXIMUM_BODY_BYTES: usize = 1024 * 1024;

fn encode(value: &Value) -> Result<Vec<u8>, PluginFault> {
    serde_json::to_vec(value).map_err(|_| fault("响应编码失败"))
}

fn error(status: u16, code: &str, message: &str) -> (u16, Value) {
    (status, json!({"error":{"code":code,"message":message}}))
}

fn page(mut value: Value, query: &str) -> Result<(u16, Value), PluginFault> {
    let params: std::collections::BTreeMap<_, _> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let offset = match params.get("pageOffset").map(|s| s.parse::<usize>()) {
        None => 0,
        Some(Ok(n)) => n,
        Some(Err(_)) => return Ok(error(400, "invalid_page", "分页位置无效，请刷新")),
    };
    let encoded = encode(&value)?;
    if encoded.len() <= PAGE_BYTES && offset == 0 {
        // 后续页遇到数据缩小也必须重新读取，不能把新快照拼到旧数据后面。
        if params.contains_key("pageSnapshot") {
            return Ok(error(409, "pagination_changed", "数据已更新，请重新读取"));
        }
        return Ok((200, value));
    }
    let snapshot = format!("{:x}", Sha256::digest(&encoded));
    if params
        .get("pageSnapshot")
        .is_some_and(|expected| expected != &snapshot)
    {
        return Ok(error(409, "pagination_changed", "数据已更新，请重新读取"));
    }
    // 这三类列表可能随账户数、保留历史或模型目录增长；报告本身受单条状态上限约束。
    let field = ["jobs", "accounts", "models"]
        .into_iter()
        .find(|field| value[*field].is_array());
    let Some(field) = field else {
        return Ok(error(
            413,
            "response_too_large",
            "响应内容过大，请缩小查询范围",
        ));
    };
    let Value::Array(rows) = value[field].take() else {
        unreachable!()
    };
    if offset >= rows.len() || (offset > 0 && !params.contains_key("pageSnapshot")) {
        return Ok(error(400, "invalid_page", "分页位置无效，请刷新"));
    }
    value[field] = json!([]);
    value["pagination"] = json!({"field":field,"nextOffset":rows.len(),"snapshot":snapshot});
    let mut bytes = encode(&value)?.len();
    let mut end = offset;
    while end < rows.len() {
        let row_bytes = encode(&rows[end])?.len() + usize::from(end > offset);
        if bytes.saturating_add(row_bytes) > PAGE_BYTES {
            break;
        }
        bytes += row_bytes;
        end += 1;
    }
    if end == offset {
        return Ok(error(
            413,
            "response_too_large",
            "单条响应内容过大，请缩小查询范围",
        ));
    }
    value[field] = Value::Array(rows[offset..end].to_vec());
    value["pagination"]["nextOffset"] = if end < rows.len() {
        json!(end)
    } else {
        Value::Null
    };
    Ok((200, value))
}

pub fn reply(
    result: Result<Value, PluginFault>,
    method: &str,
    query: &str,
) -> Result<TypedReply<ManagementResponse>, PluginFault> {
    let (mut status, value) = match result {
        Ok(value) if method == "GET" => page(value, query)?,
        Ok(value) => (200, value),
        Err(e) => (400, json!({"error":{"message":e.message}})),
    };
    let mut payload = encode(&value)?;
    if payload.len() > MAXIMUM_BODY_BYTES {
        let (code, value) = error(413, "response_too_large", "响应内容过大，请缩小查询范围");
        status = code;
        payload = encode(&value)?;
    }
    Ok(TypedReply::new(ManagementResponse {
        status,
        content_type: "application/json".into(),
        headers: vec![],
    })
    .with_payload(payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_pages_preserve_all_rows_and_metadata() {
        let rows: Vec<_> = (0..1200)
            .map(|id| json!({"id":id,"message":"检测详情".repeat(200)}))
            .collect();
        let full = json!({"jobs":rows,"revision":"test:1","banks":{"active":"builtin"}});
        let mut query = String::new();
        let mut collected = vec![];
        loop {
            let (status, value) = page(full.clone(), &query).unwrap();
            assert_eq!(status, 200);
            assert!(encode(&value).unwrap().len() <= PAGE_BYTES);
            assert_eq!(value["banks"], full["banks"]);
            collected.extend(value["jobs"].as_array().unwrap().iter().cloned());
            let Some(offset) = value["pagination"]["nextOffset"].as_u64() else {
                break;
            };
            query = format!(
                "pageOffset={offset}&pageSnapshot={}",
                value["pagination"]["snapshot"].as_str().unwrap()
            );
        }
        assert_eq!(collected, rows);
    }

    #[test]
    fn changed_snapshot_and_invalid_offsets_are_rejected() {
        let full = json!({"accounts":vec![json!({"name":"账户".repeat(1000)});200]});
        let (_, first) = page(full.clone(), "").unwrap();
        let snapshot = first["pagination"]["snapshot"].as_str().unwrap();
        let query = format!("pageOffset=10&pageSnapshot={snapshot}");
        let mut changed = full.clone();
        changed["accounts"][0]["name"] = json!("changed");
        assert_eq!(page(changed, &query).unwrap().0, 409);
        assert_eq!(page(json!({"accounts":[]}), &query).unwrap().0, 409);
        for query in ["pageOffset=bad", "pageOffset=200", "pageOffset=1"] {
            assert_eq!(page(full.clone(), query).unwrap().0, 400);
        }
    }

    #[test]
    fn oversized_single_record_returns_a_bounded_error() {
        let (status, value) = page(
            json!({"jobs":[{"message":"x".repeat(MAXIMUM_BODY_BYTES)}]}),
            "",
        )
        .unwrap();
        assert_eq!(status, 413);
        assert!(encode(&value).unwrap().len() < 1024);
    }
}
