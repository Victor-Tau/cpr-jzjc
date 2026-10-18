use chrono::{Duration, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Filter {
    pub plan: String,
    pub status: String,
    pub group: String,
    pub keyword: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Schedule {
    pub enabled: bool,
    pub kind: String,
    pub interval_minutes: u32,
    pub time: String,
    pub timezone: String,
    pub scope: String,
    pub account_ids: Vec<String>,
    pub filter: Filter,
}
impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: "interval".into(),
            interval_minutes: 1440,
            time: "03:00".into(),
            timezone: "UTC".into(),
            scope: "filter".into(),
            account_ids: vec![],
            filter: Filter::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Config {
    pub request_model: String,
    pub key_id: String,
    pub temperature: Option<f64>,
    pub reasoning_effort: String,
    pub max_output_tokens: u32,
    pub timeout_seconds: u64,
    pub degraded_models: Vec<String>,
    pub aliases: BTreeMap<String, String>,
    pub low_confidence_percent: f64,
    pub degraded_percent: f64,
    pub confirmation_rounds: u32,
    pub low_confidence_retries: u32,
    pub retry_delay_seconds: u32,
    pub auto_disable: bool,
    pub retention_days: u32,
    pub schedule: Schedule,
    pub auto_check_updates: bool,
    pub update_interval_hours: u32,
    pub mirror_prefix: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            request_model: "gpt-6.1-sol".into(),
            key_id: String::new(),
            temperature: None,
            reasoning_effort: "low".into(),
            max_output_tokens: 4096,
            timeout_seconds: 120,
            degraded_models: vec!["gpt-5.6-luna".into()],
            aliases: BTreeMap::new(),
            low_confidence_percent: 60.0,
            degraded_percent: 80.0,
            confirmation_rounds: 3,
            low_confidence_retries: 2,
            retry_delay_seconds: 30,
            auto_disable: false,
            retention_days: 30,
            schedule: Schedule::default(),
            auto_check_updates: false,
            update_interval_hours: 24,
            mirror_prefix: String::new(),
        }
    }
}
impl Config {
    pub fn validate(&self, model_ids: &[String]) -> Result<(), String> {
        if self.request_model.trim().is_empty()
            || self.request_model.len() > 128
            || self.key_id.len() > 128
        {
            return Err("请求模型或Key无效".into());
        }
        if self
            .temperature
            .is_some_and(|v| !v.is_finite() || !(0.0..=2.0).contains(&v))
        {
            return Err("temperature须为0至2或留空".into());
        }
        if !["", "none", "minimal", "low", "medium", "high", "xhigh"]
            .contains(&self.reasoning_effort.as_str())
        {
            return Err("推理强度无效".into());
        }
        if !(512..=16384).contains(&self.max_output_tokens) {
            return Err("输出上限须为512至16384".into());
        }
        if !self.low_confidence_percent.is_finite()
            || !self.degraded_percent.is_finite()
            || !(0.0..=100.0).contains(&self.low_confidence_percent)
            || !(0.0..=100.0).contains(&self.degraded_percent)
            || self.low_confidence_percent > self.degraded_percent
        {
            return Err("概率阈值须为0至100，低置信度阈值不能高于降智阈值".into());
        }
        if !(1..=5).contains(&self.confirmation_rounds)
            || self.low_confidence_retries > 5
            || self.retry_delay_seconds > 3600
        {
            return Err("确认轮数须为1至5，低置信度复测预算0至5，等待至多3600秒".into());
        }
        if self.degraded_models.is_empty()
            || self.degraded_models.len() > 32
            || self.aliases.len() > 64
        {
            return Err("请配置1至32个降级模型，别名最多64条".into());
        }
        let mut referenced = self.degraded_models.clone();
        referenced.extend(self.aliases.values().cloned());
        if let Some(missing) = referenced.iter().find(|m| !model_ids.contains(m)) {
            return Err(format!("当前指纹库不存在模型：{missing}"));
        }
        if self.aliases.keys().any(|k| k.is_empty() || k.len() > 128) {
            return Err("别名名称无效".into());
        }
        if !(1..=365).contains(&self.retention_days)
            || !(1..=720).contains(&self.update_interval_hours)
        {
            return Err("历史保留须为1至365天，更新检查间隔为1至720小时".into());
        }
        if !self.mirror_prefix.is_empty() {
            let u = url::Url::parse(&self.mirror_prefix).map_err(|_| "镜像前缀须为HTTPS地址")?;
            if u.scheme() != "https"
                || !u.username().is_empty()
                || u.password().is_some()
                || u.query().is_some()
                || u.fragment().is_some()
            {
                return Err("镜像前缀须为无认证、查询或片段的HTTPS地址".into());
            }
        }
        let s = &self.schedule;
        if !["interval", "daily"].contains(&s.kind.as_str())
            || !["filter", "ids"].contains(&s.scope.as_str())
            || !(1..=43200).contains(&s.interval_minutes)
            || s.account_ids.len() > 1000
        {
            return Err("定时方式、范围或间隔无效".into());
        }
        s.timezone.parse::<Tz>().map_err(|_| "IANA时区无效")?;
        parse_time(&s.time)?;
        if s.enabled && self.key_id.is_empty() {
            return Err("开启定时检测前请选择计量Key".into());
        }
        if s.enabled && s.scope == "ids" && s.account_ids.is_empty() {
            return Err("请指定定时检测账户".into());
        }
        Ok(())
    }
    pub fn canonical<'a>(&'a self, model: &'a str) -> &'a str {
        self.aliases.get(model).map(String::as_str).unwrap_or(model)
    }
}

fn parse_time(time: &str) -> Result<(u32, u32), String> {
    let (h, m) = time.split_once(':').ok_or("每日时间须为HH:MM")?;
    let (h, m): (u32, u32) = (
        h.parse().map_err(|_| "时间无效")?,
        m.parse().map_err(|_| "时间无效")?,
    );
    if h > 23 || m > 59 {
        return Err("时间无效".into());
    }
    Ok((h, m))
}
pub fn next_due(s: &Schedule, after_ms: u64) -> u64 {
    if s.kind == "interval" {
        return after_ms + u64::from(s.interval_minutes) * 60000;
    }
    let tz = s.timezone.parse::<Tz>().unwrap_or(chrono_tz::UTC);
    let after = chrono::DateTime::from_timestamp_millis(after_ms as i64).unwrap_or_else(Utc::now);
    let local = after.with_timezone(&tz);
    let (h, m) = parse_time(&s.time).unwrap_or((3, 0));
    // Iterate instants, not wall times: skipped DST minutes run at the first valid minute after them.
    let mut cursor =
        after.with_second(0).unwrap().with_nanosecond(0).unwrap() + Duration::minutes(1);
    for _ in 0..3000 {
        let t = cursor.with_timezone(&tz);
        if (t.date_naive() == local.date_naive() && (local.hour(), local.minute()) < (h, m)
            || t.date_naive() > local.date_naive())
            && (t.hour(), t.minute()) >= (h, m)
        {
            return cursor.timestamp_millis() as u64;
        }
        cursor += Duration::minutes(1);
    }
    after_ms + 86400000
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub provider: String,
    pub enabled: bool,
    pub status: String,
    pub plan_type: Option<String>,
    pub groups: Vec<Group>,
    pub supported: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
}
impl Account {
    pub fn matches(&self, f: &Filter) -> bool {
        (f.plan.is_empty()
            || self
                .plan_type
                .as_deref()
                .unwrap_or("unknown")
                .eq_ignore_ascii_case(&f.plan))
            && (f.status.is_empty()
                || if f.status == "disabled" {
                    !self.enabled
                } else {
                    self.enabled && self.status == f.status
                })
            && (f.group.is_empty()
                || if f.group == "__ungrouped__" {
                    self.groups.is_empty()
                } else {
                    self.groups.iter().any(|g| g.id == f.group)
                })
            && (f.keyword.trim().is_empty()
                || format!(
                    "{} {} {}",
                    self.name,
                    self.email.as_deref().unwrap_or(""),
                    self.id
                )
                .to_lowercase()
                .contains(&f.keyword.trim().to_lowercase()))
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub authentication_kind: String,
    pub upstream_user_id: Option<String>,
    pub upstream_account_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Challenge {
    pub id: String,
    pub prompt: String,
    #[serde(rename = "expected_count")]
    pub expected_count: u32,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub numbers: Vec<u32>,
    pub expected_count: u32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub model: String,
    pub label: String,
    pub family: String,
    pub probability: f64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Score {
    pub prediction: String,
    pub probability: f64,
    pub results: Vec<Candidate>,
    pub families: serde_json::Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    pub number: u32,
    pub verdict: String,
    pub score: Score,
    pub finished_at: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub request_id: Option<String>,
    pub at: u64,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub source: String,
    pub status: String,
    pub verdict: String,
    pub message: String,
    pub created_at: u64,
    pub finished_at: Option<u64>,
    pub next_at: u64,
    pub config: Config,
    pub bank_hash: Option<String>,
    pub bank_commit: Option<String>,
    pub engine_commit: String,
    pub identity: Option<Identity>,
    pub rounds: Vec<Round>,
    pub samples: Vec<Sample>,
    pub challenges: Vec<Challenge>,
    pub attempts: Vec<Attempt>,
    pub round_attempts: u32,
    pub consecutive: u32,
    pub low_retries_used: u32,
    pub pending_probe: bool,
    pub disable_state: String,
}
impl Job {
    pub fn active(&self) -> bool {
        ["queued", "running"].contains(&self.status.as_str())
    }
    pub fn finish(&mut self, status: &str, verdict: &str, message: impl Into<String>, now: u64) {
        self.status = status.into();
        self.verdict = verdict.into();
        self.message = message.into();
        self.finished_at = Some(now);
        self.pending_probe = false;
        self.samples.clear();
        self.challenges.clear();
    }
    pub fn classify(&mut self, score: &Score) -> &'static str {
        if score.probability * 100.0 < self.config.low_confidence_percent {
            self.consecutive = 0;
            return "low_confidence";
        }
        let cfg = &self.config;
        if !cfg
            .degraded_models
            .iter()
            .any(|m| m == cfg.canonical(&cfg.request_model))
            && cfg.degraded_models.contains(&score.prediction)
            && score.probability * 100.0 >= cfg.degraded_percent
        {
            self.consecutive += 1;
            if self.consecutive >= cfg.confirmation_rounds {
                "degraded"
            } else {
                "suspected"
            }
        } else {
            self.consecutive = 0;
            if score.probability * 100.0 < cfg.degraded_percent {
                "inconclusive"
            } else {
                "no_match"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    pub fn job() -> Job {
        serde_json::from_value(serde_json::json!({"id":"j","accountId":"a","accountName":"a","source":"manual","status":"running","verdict":"","message":"","createdAt":0,"finishedAt":null,"nextAt":0,"config":Config::default(),"bankHash":null,"bankCommit":null,"engineCommit":"x","identity":null,"rounds":[],"samples":[],"challenges":[],"attempts":[],"roundAttempts":0,"consecutive":0,"lowRetriesUsed":0,"pendingProbe":false,"disableState":"none"})).unwrap()
    }
    fn score(model: &str, p: f64) -> Score {
        Score {
            prediction: model.into(),
            probability: p,
            results: vec![],
            families: serde_json::json!([]),
        }
    }
    #[test]
    fn thresholds_and_confirmation() {
        let mut j = job();
        assert_eq!(j.classify(&score("gpt-5.6-luna", 0.599)), "low_confidence");
        assert_eq!(j.classify(&score("gpt-5.6-luna", 0.60)), "inconclusive");
        assert_eq!(j.classify(&score("gpt-5.6-luna", 0.80)), "suspected");
        assert_eq!(j.classify(&score("gpt-5.6-luna", 0.90)), "suspected");
        assert_eq!(j.classify(&score("gpt-5.6-luna", 0.95)), "degraded");
        assert_eq!(j.classify(&score("gpt-6-sol", 0.95)), "no_match");
        assert_eq!(j.consecutive, 0);
    }
    #[test]
    fn request_alias_exemption() {
        let mut j = job();
        j.config
            .aliases
            .insert("my-luna".into(), "gpt-5.6-luna".into());
        j.config.request_model = "my-luna".into();
        assert_eq!(j.classify(&score("gpt-5.6-luna", 1.0)), "no_match");
    }
    #[test]
    fn daily_dst_and_interval() {
        let s = Schedule::default();
        assert_eq!(next_due(&s, 1000), 86401000);
        let s = Schedule {
            kind: "daily".into(),
            timezone: "America/Los_Angeles".into(),
            time: "02:30".into(),
            ..s
        };
        let now = Utc
            .with_ymd_and_hms(2026, 3, 8, 9, 0, 0)
            .unwrap()
            .timestamp_millis() as u64;
        assert_eq!(
            next_due(&s, now),
            Utc.with_ymd_and_hms(2026, 3, 8, 10, 0, 0)
                .unwrap()
                .timestamp_millis() as u64
        );
        let s = Schedule {
            time: "01:30".into(),
            ..s
        };
        let now = Utc
            .with_ymd_and_hms(2026, 11, 1, 8, 30, 0)
            .unwrap()
            .timestamp_millis() as u64;
        assert_eq!(
            next_due(&s, now),
            Utc.with_ymd_and_hms(2026, 11, 2, 9, 30, 0)
                .unwrap()
                .timestamp_millis() as u64
        );
    }
}
