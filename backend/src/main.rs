mod banks;
mod domain;
mod engine;
mod host;
mod manager;
mod progress;
use gateway_plugin_sdk::{
    Manifest, PluginFault,
    call::management::*,
    client::{Empty, PluginBuilder, PluginSession, SessionConfig, TypedCall, TypedReply, methods},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;

fn registration() -> ManagementRegistration {
    let routes = [
        ("GET", "api/bootstrap"),
        ("GET", "api/accounts"),
        ("GET", "api/models"),
        ("GET", "api/jobs"),
        ("GET", "api/progress"),
        ("GET", "api/report"),
        ("POST", "api/config"),
        ("POST", "api/config/preview"),
        ("POST", "api/submit"),
        ("POST", "api/cancel"),
        ("POST", "api/delete"),
        ("POST", "api/retry-disable"),
        ("POST", "api/retry-group"),
        ("POST", "api/banks/check"),
        ("POST", "api/banks/apply"),
        ("POST", "api/banks/rollback"),
        ("POST", "api/banks/builtin"),
    ]
    .into_iter()
    .map(|(m, p)| ManagementRoute {
        method: m.into(),
        path: p.into(),
        request_content_types: if m == "GET" {
            vec![]
        } else {
            vec!["application/json".into()]
        },
        response_content_types: vec!["application/json".into()],
    })
    .collect();
    ManagementRegistration {
        routes,
        resources: ["web/index.html", "web/app.js", "web/app.css"]
            .into_iter()
            .map(|p| ManagementResource {
                path: p.into(),
                public: false,
            })
            .collect(),
        pages: vec![ManagementPage {
            id: "modeltrace-health".into(),
            title: "账号降智检测".into(),
            description: Some("ModelTrace 指纹检测与定时检查".into()),
            entry: "web/index.html".into(),
            icon: None,
        }],
        callbacks: vec![],
    }
}
async fn handle(
    m: &mut manager::Manager,
    call: TypedCall<ManagementRequest>,
) -> Result<Value, PluginFault> {
    let h = &call.host;
    m.load(h).await?;
    let query = |key: &str| {
        url::form_urlencoded::parse(call.request.query.as_bytes())
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    let body: Value = if call.payload.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&call.payload).map_err(|_| host::fault("请求JSON格式无效"))?
    };
    match (call.request.method.as_str(), call.request.path.as_str()) {
        ("GET", "api/bootstrap") => Ok(
            json!({"config":m.control.config,"configRevision":m.control.config_revision,"banks":m.bank_status(),"keys":host::keys(h).await?,"groups":host::groups(h).await?,"jobs":m.summaries(),"nextDue":m.control.next_due,"scheduleError":m.control.schedule_error,"lastScheduleAt":m.control.last_schedule_at}),
        ),
        ("GET", "api/accounts") => {
            let offset = query("offset").parse::<usize>().unwrap_or(0);
            let rows = host::accounts(h).await?;
            let end = (offset + 500).min(rows.len());
            if offset > rows.len() {
                return Err(host::fault("账户分页位置无效，请刷新"));
            }
            Ok(
                json!({"accounts":&rows[offset..end],"nextOffset":if end<rows.len(){Some(end)}else{None}}),
            )
        }
        ("GET", "api/models") => {
            let models:gateway_plugin_sdk::call::host::ModelListResult=host::meta(h,"host.models.list",json!({"client_key_id":query("keyId"),"protocol":"openai","client_version":"modeltrace-health/1.4.3"})).await?;
            Ok(json!({"models":models.models}))
        }
        ("GET", "api/progress") => m
            .progress
            .read(Some(query("since").as_str()), true)
            .ok_or_else(|| host::fault("进度尚未准备")),
        ("GET", "api/jobs") => Ok(
            json!({"jobs":m.summaries(),"banks":m.bank_status(),"nextDue":m.control.next_due,"scheduleError":m.control.schedule_error}),
        ),
        ("GET", "api/report") => {
            let id = query("id");
            let j = &m
                .jobs
                .iter()
                .find(|(j, _)| j.id == id)
                .ok_or_else(|| host::fault("报告不存在"))?
                .0;
            let mut value = serde_json::to_value(j).unwrap();
            value.as_object_mut().unwrap().remove("samples");
            value.as_object_mut().unwrap().remove("challenges");
            Ok(value)
        }
        ("POST", "api/config") => {
            let cfg = serde_json::from_value(body["config"].clone())
                .map_err(|_| host::fault("配置字段格式无效"))?;
            m.set_config(
                h,
                cfg,
                body["revision"]
                    .as_u64()
                    .ok_or_else(|| host::fault("缺少配置版本"))?,
            )
            .await?;
            Ok(json!({"revision":m.control.config_revision,"config":m.control.config}))
        }
        ("POST", "api/config/preview") => {
            let cfg: domain::Config = serde_json::from_value(body["config"].clone())
                .map_err(|_| host::fault("配置字段格式无效"))?;
            cfg.validate(
                &m.active_bank()
                    .models
                    .iter()
                    .map(|model| model.id.clone())
                    .collect::<Vec<_>>(),
            )
            .map_err(host::fault)?;
            Ok(json!({"config":cfg}))
        }
        ("POST", "api/submit") => {
            let ids: Vec<String> = serde_json::from_value(body["accountIds"].clone())
                .map_err(|_| host::fault("账户列表格式错误"))?;
            let id = body["requestId"]
                .as_str()
                .ok_or_else(|| host::fault("缺少提交ID"))?;
            Ok(json!({"submitted":m.submit(h,ids,id,"manual").await?}))
        }
        ("POST", "api/cancel") => {
            m.cancel(h, body["id"].as_str()).await?;
            Ok(json!({"ok":true}))
        }
        ("POST", "api/delete") => {
            m.delete_job(
                h,
                body["id"]
                    .as_str()
                    .ok_or_else(|| host::fault("缺少任务ID"))?,
            )
            .await?;
            Ok(json!({"ok":true}))
        }
        ("POST", "api/retry-disable") => {
            let id = body["id"].as_str().unwrap_or("");
            let i = m
                .jobs
                .iter()
                .position(|(j, _)| j.id == id && j.disable_state == "failed")
                .ok_or_else(|| host::fault("没有可重试的停用操作"))?;
            m.disable(h, i).await?;
            Ok(json!({"ok":true}))
        }
        ("POST", "api/retry-group") => {
            let id = body["id"].as_str().unwrap_or("");
            let i = m
                .jobs
                .iter()
                .position(|(j, _)| j.id == id && j.group_state == "failed")
                .ok_or_else(|| host::fault("没有可重试的移组操作"))?;
            m.move_group(h, i).await?;
            Ok(json!({"ok":true}))
        }
        ("POST", "api/banks/check") => {
            m.check_update(h).await?;
            Ok(m.bank_status())
        }
        ("POST", "api/banks/apply") => {
            m.apply_update(h).await?;
            Ok(m.bank_status())
        }
        ("POST", "api/banks/rollback") => {
            m.switch_bank(h, false).await?;
            Ok(m.bank_status())
        }
        ("POST", "api/banks/builtin") => {
            m.switch_bank(h, true).await?;
            Ok(m.bank_status())
        }
        _ => Err(host::fault("管理接口不存在")),
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = PluginSession::accept(
        tokio::io::stdin(),
        tokio::io::stdout(),
        SessionConfig::default(),
    )
    .await?;
    let manifest = Manifest::from_author_slice(include_bytes!("../../plugin.json"))?;
    if session.handshake().plugin_id != manifest.plugin_id()?
        || session.handshake().contributes != manifest.contributes
    {
        return Err("插件握手身份不匹配".into());
    }
    let progress = Arc::new(progress::Progress::new());
    let manager = Arc::new(Mutex::new(manager::Manager::new(progress.clone())));
    let background = manager.clone();
    let shutdown = manager.clone();
    let plugin = PluginBuilder::from_json(include_bytes!("../../plugin.json"))?
        .management(registration(), move |call| {
            let manager = manager.clone();
            let progress = progress.clone();
            async move {
                let query = |key: &str| {
                    url::form_urlencoded::parse(call.request.query.as_bytes())
                        .find(|(k, _)| k == key)
                        .map(|(_, v)| v.into_owned())
                        .unwrap_or_default()
                };
                let cached = if call.request.method == "GET" {
                    match call.request.path.as_str() {
                        "api/progress" => progress.read(Some(query("since").as_str()), true),
                        "api/jobs" => progress.read(None, false),
                        "api/report" => progress.report(&query("id")),
                        _ => None,
                    }
                } else {
                    None
                };
                let result = if let Some(value) = cached {
                    Ok(value)
                } else if call.request.method == "POST" && call.request.path == "api/cancel" {
                    match serde_json::from_slice::<Value>(&call.payload) {
                        Ok(body) => match progress.cancel(body["id"].as_str()) {
                            Ok(()) => {
                                if let Ok(mut m) = manager.try_lock() {
                                    m.cancel(&call.host, body["id"].as_str())
                                        .await
                                        .map(|()| json!({"ok":true}))
                                } else {
                                    Ok(json!({"ok":true}))
                                }
                            }
                            Err(error) => Err(error),
                        },
                        Err(_) => Err(host::fault("请求JSON格式无效")),
                    }
                } else {
                    let mut m = manager.lock().await;
                    let result = handle(&mut m, call).await;
                    if result.is_err() {
                        m.loaded = false;
                    }
                    result
                };
                let (status, v) = match result {
                    Ok(v) => (200, v),
                    Err(e) => (400, json!({"error":{"message":e.message}})),
                };
                Ok(TypedReply::new(ManagementResponse {
                    status,
                    content_type: "application/json".into(),
                    headers: vec![],
                })
                .with_payload(serde_json::to_vec(&v).map_err(|_| host::fault("响应编码失败"))?))
            }
        })?
        .on(methods::RECONCILE, move |call| {
            let manager = background.clone();
            async move {
                if let Ok(mut m) = manager.try_lock()
                    && let Err(e) = m
                        .tick(
                            &call.host,
                            std::time::Duration::from_millis(call.context.timeout_ms),
                        )
                        .await
                {
                    m.loaded = false;
                    return Err(e);
                }
                Ok(TypedReply::new(Empty {}))
            }
        })?
        .build()?;
    let result = session.run(plugin).await;
    shutdown
        .lock()
        .await
        .engine
        .cleanup()
        .map_err(|e| std::io::Error::other(e.message))?;
    result?;
    Ok(())
}
