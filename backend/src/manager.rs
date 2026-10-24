use crate::progress::Progress;
use crate::{
    banks::{self, Bank, Update},
    domain::*,
    engine::Engine,
    host::{self, ExecutionError, ExecutionResult, fault, now},
};
use gateway_plugin_sdk::{PluginFault, client::HostClient};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::task::JoinSet;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    pub config: Config,
    pub config_revision: u64,
    pub job_ids: Vec<String>,
    pub banks: Vec<Bank>,
    pub active_bank: String,
    pub previous_bank: Option<String>,
    pub pending_bank: Option<Bank>,
    pub update: Update,
    pub next_due: Option<u64>,
    pub schedule_error: Option<String>,
    pub last_schedule_at: Option<u64>,
    pub pending_schedule: Option<String>,
}
impl Default for Control {
    fn default() -> Self {
        let b = banks::builtin();
        Self {
            config: Config::default(),
            config_revision: 0,
            job_ids: vec![],
            active_bank: b.hash.clone(),
            banks: vec![b],
            previous_bank: None,
            pending_bank: None,
            update: Update::default(),
            next_due: None,
            schedule_error: None,
            last_schedule_at: None,
            pending_schedule: None,
        }
    }
}
pub struct Manager {
    pub control: Control,
    version: Option<u64>,
    pub jobs: Vec<(Job, Option<u64>)>,
    pub loaded: bool,
    recovered: bool,
    reload_required: Arc<AtomicBool>,
    pub engine: Engine,
    pub progress: Arc<Progress>,
    deadline: Option<tokio::time::Instant>,
    request_window: std::time::Duration,
    last_job_source: Option<String>,
    score_bank: Option<(String, std::path::PathBuf)>,
}
impl Manager {
    pub fn new(progress: Arc<Progress>) -> Self {
        Self {
            control: Control::default(),
            version: None,
            jobs: vec![],
            loaded: false,
            recovered: false,
            reload_required: Arc::new(AtomicBool::new(false)),
            engine: Engine::new(),
            progress,
            deadline: None,
            request_window: std::time::Duration::ZERO,
            last_job_source: None,
            score_bank: None,
        }
    }
    /// Shared with the RPC layer so an interrupted maintenance callback can
    /// force a clean reload on the next management or maintenance call.
    pub fn reload_flag(&self) -> Arc<AtomicBool> {
        self.reload_required.clone()
    }
    pub async fn save(&mut self, host: &HostClient) -> Result<(), PluginFault> {
        self.version =
            Some(host::state_put(host, "control", "main", &self.control, self.version).await?);
        self.publish(None);
        Ok(())
    }
    fn publish(&self, index: Option<usize>) {
        let reports = self
            .jobs
            .iter()
            .enumerate()
            .filter(|(i, _)| index.is_none_or(|index| *i == index))
            .map(|(_, (j, _))| (j.id.clone(), serde_json::to_value(j).unwrap()));
        self.progress.publish(
            self.summaries(),
            self.bank_status(),
            self.control.next_due,
            self.control.schedule_error.clone(),
            reports,
        );
    }
    async fn save_job(&mut self, host: &HostClient, i: usize) -> Result<(), PluginFault> {
        let (j, v) = &mut self.jobs[i];
        match host::state_put(host, "jobs", &j.id, j, *v).await {
            Ok(version) => *v = Some(version),
            Err(error) => {
                self.loaded = false;
                return Err(error);
            }
        }
        self.publish(Some(i));
        Ok(())
    }
    pub async fn load(&mut self, host: &HostClient) -> Result<(), PluginFault> {
        // A cancelled RPC future drops the tick without running its error path.
        // The RPC layer raises this flag so the next call re-reads persisted
        // state instead of trusting in-memory mutations that never reached the
        // host.
        if self.loaded && !self.reload_required.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        self.loaded = false;
        self.jobs.clear();
        if let Some(r) = host::state_get(host, "control", "main").await? {
            self.control =
                serde_json::from_value(r.value).map_err(|_| fault("插件状态格式错误"))?;
            self.version = Some(r.version);
        } else {
            self.control = Control::default();
            self.version = None;
        }
        // 1.7.1 起历史固定保留最近三天；旧配置中的天数在载入时归一化。
        self.control.config.retention_days = HISTORY_RETENTION_DAYS;
        for id in self.control.job_ids.clone() {
            if let Some(r) = host::state_get(host, "jobs", &id).await? {
                let mut job: Job =
                    serde_json::from_value(r.value).map_err(|_| fault("历史任务格式错误"))?;
                if job.active() && (!self.recovered || job.pending_probe) {
                    job.finish(
                        "interrupted",
                        "inconclusive",
                        "上一次探针执行被中断，结果不确定；请重新检测",
                        now(),
                    );
                }
                if job.disable_state == "attempting" {
                    job.disable_state = "failed".into();
                    job.disable_error = Some("操作时插件中断，请检查宿主状态或重试".into());
                    job.action_message();
                }
                if job.group_state == "attempting" {
                    job.group_state = "failed".into();
                    job.group_error = Some("操作时插件中断，请检查宿主状态或重试".into());
                    job.action_message();
                }
                self.jobs.push((job, Some(r.version)));
            }
        }
        for i in 0..self.jobs.len() {
            if self.jobs[i].0.status == "interrupted"
                || self.jobs[i].0.disable_state == "failed"
                || self.jobs[i].0.group_state == "failed"
            {
                self.save_job(host, i).await?;
            }
        }
        self.control.job_ids = self.jobs.iter().map(|(j, _)| j.id.clone()).collect();
        if let Some(p) = self.control.pending_bank.take()
            && !self.control.banks.iter().any(|b| b.hash == p.hash)
        {
            banks::remove(host, &p).await?;
        }
        self.control.pending_schedule = None;
        self.save(host).await?;
        self.loaded = true;
        self.recovered = true;
        Ok(())
    }
    /// Clears execution boundaries left behind by an RPC cancellation. A probe
    /// whose outcome was never collected is marked interrupted instead of
    /// remaining pending and being skipped forever; idempotent account actions
    /// return to pending so the next callback can verify and retry them.
    async fn recover_interrupted(&mut self, host: &HostClient) -> Result<(), PluginFault> {
        let mut changed = vec![];
        for i in 0..self.jobs.len() {
            let j = &mut self.jobs[i].0;
            let mut dirty = false;
            if j.pending_probe {
                j.finish(
                    "interrupted",
                    "inconclusive",
                    "上一次探针执行被中断，结果不确定；请重新检测",
                    now(),
                );
                dirty = true;
            }
            if j.disable_state == "attempting" {
                j.disable_state = "pending".into();
                j.disable_error = None;
                dirty = true;
            }
            if j.group_state == "attempting" {
                j.group_state = "pending".into();
                j.group_error = None;
                dirty = true;
            }
            if dirty {
                j.action_message();
                changed.push(i);
            }
        }
        for i in changed {
            self.save_job(host, i).await?;
        }
        Ok(())
    }
    pub fn bank(&self, hash: &str) -> Result<Bank, PluginFault> {
        self.control
            .banks
            .iter()
            .find(|b| b.hash == hash)
            .cloned()
            .ok_or_else(|| fault("任务引用的指纹库不存在"))
    }
    pub fn active_bank(&self) -> Bank {
        self.control
            .banks
            .iter()
            .find(|b| b.hash == self.control.active_bank)
            .unwrap()
            .clone()
    }
    pub fn bank_status(&self) -> Value {
        json!({"active":self.active_bank(),"previous":self.control.previous_bank.as_ref().and_then(|h|self.control.banks.iter().find(|b|&b.hash==h)),"builtin":banks::builtin(),"update":self.control.update,"engineCommit":banks::engine_commit()})
    }
    pub fn summaries(&self) -> Vec<Value> {
        self.jobs.iter().rev().map(|(j,_)|json!({"id":j.id,"accountId":j.account_id,"accountName":j.account_name,"source":j.source,"status":j.status,"verdict":j.verdict,"message":j.message,"createdAt":j.created_at,"finishedAt":j.finished_at,"roundCount":j.rounds.len(),"sampleCount":j.samples.len(),"consecutive":j.consecutive,"roundAttempts":j.round_attempts,"attemptCount":j.attempts.len(),"nextAt":j.next_at,"prediction":j.rounds.last().map(|r|&r.score.prediction),"probability":j.rounds.last().map(|r|r.score.probability),"bankHash":j.bank_hash,"disableState":j.disable_state,"groupState":j.group_state})).collect()
    }
    async fn prepare_score(
        &mut self,
        host: &HostClient,
        i: usize,
    ) -> Result<std::path::PathBuf, PluginFault> {
        let bank = self.bank(self.jobs[i].0.bank_hash.as_ref().unwrap())?;
        if let Some((hash, path)) = &self.score_bank
            && hash == &bank.hash
        {
            return Ok(path.clone());
        }
        let bytes = banks::read(host, &bank).await?;
        let path = self.engine.write_bank(&bank.hash, &bytes).await?;
        let _: Value = self
            .engine
            .call(json!({"op":"prepare_score","path":path}))
            .await?;
        self.score_bank = Some((bank.hash, path.clone()));
        Ok(path)
    }
    pub async fn set_config(
        &mut self,
        host: &HostClient,
        mut config: Config,
        revision: u64,
    ) -> Result<(), PluginFault> {
        if revision != self.control.config_revision {
            return Err(fault("配置已由其他页面更新，请刷新后重试"));
        }
        config.retention_days = HISTORY_RETENTION_DAYS;
        config
            .validate(
                &self
                    .active_bank()
                    .models
                    .iter()
                    .map(|m| m.id.clone())
                    .collect::<Vec<_>>(),
            )
            .map_err(fault)?;
        if !config.key_id.is_empty()
            && !host::keys(host)
                .await?
                .iter()
                .any(|k| k.id == config.key_id)
        {
            return Err(fault("所选Key不存在或已停用"));
        }
        Self::validate_target_group(host, &config).await?;
        let schedule_changed = serde_json::to_value(&config.schedule).unwrap()
            != serde_json::to_value(&self.control.config.schedule).unwrap();
        if schedule_changed {
            self.control.next_due = if config.schedule.enabled {
                Some(next_due(&config.schedule, now()))
            } else {
                None
            };
        }
        self.control.config = config;
        self.control.config_revision += 1;
        self.save(host).await
    }
    pub async fn submit(
        &mut self,
        host: &HostClient,
        ids: Vec<String>,
        request: &str,
        source: &str,
    ) -> Result<Vec<String>, PluginFault> {
        if request.is_empty() || request.len() > 128 || ids.is_empty() || ids.len() > 500 {
            return Err(fault("提交ID无效，或账户数量不在1至500之间"));
        }
        let mut unique = BTreeSet::new();
        if ids.iter().any(|id| id.len() > 128 || !unique.insert(id)) {
            return Err(fault("账户ID重复或过长"));
        }
        let cfg = self.control.config.clone();
        cfg.validate(
            &self
                .active_bank()
                .models
                .iter()
                .map(|m| m.id.clone())
                .collect::<Vec<_>>(),
        )
        .map_err(fault)?;
        if cfg.key_id.is_empty() {
            return Err(fault("请先在参数配置页选择计量Key"));
        }
        Self::validate_target_group(host, &cfg).await?;
        let rows = host::accounts(host).await?;
        let mut selected = vec![];
        for id in &ids {
            let legacy = format!(
                "j-{}",
                &banks::hash(format!("{request}:{id}").as_bytes())[..32]
            );
            // Preserve replay IDs from older releases while separating new
            // requests by source, even if a manual request uses a schedule ID.
            let existing = if self
                .jobs
                .iter()
                .any(|(j, _)| j.id == legacy && j.source == source)
            {
                legacy
            } else {
                format!(
                    "j-{}",
                    &banks::hash(format!("{source}:{request}:{id}").as_bytes())[..32]
                )
            };
            if self.control.job_ids.contains(&existing) {
                selected.push((existing, None));
                continue;
            }
            let a = rows
                .iter()
                .find(|a| a.id == *id)
                .ok_or_else(|| fault(format!("账户已不存在：{id}")))?;
            if !a.supported || !a.enabled {
                return Err(fault(format!("账户未启用或不支持检测：{}", a.name)));
            }
            if self
                .jobs
                .iter()
                .any(|(j, _)| j.account_id == *id && j.source == source && j.active())
            {
                if source == "schedule" {
                    continue;
                }
                return Err(fault(format!("账户已有手动检测任务：{}", a.name)));
            }
            selected.push((existing, Some(a.clone())));
        }
        self.prune(host).await?;
        let mut submitted = vec![];
        for (id, account) in selected {
            submitted.push(id.clone());
            let Some(a) = account else {
                continue;
            };
            let job = Job {
                id: id.clone(),
                account_id: a.id,
                account_name: a.email.filter(|s| !s.trim().is_empty()).unwrap_or(a.name),
                source: source.into(),
                status: "queued".into(),
                verdict: String::new(),
                message: "等待检测".into(),
                created_at: now(),
                finished_at: None,
                next_at: 0,
                config: cfg.clone(),
                bank_hash: None,
                bank_commit: None,
                engine_commit: banks::engine_commit(),
                identity: None,
                rounds: vec![],
                samples: vec![],
                challenges: vec![],
                attempts: vec![],
                round_attempts: 0,
                consecutive: 0,
                low_retries_used: 0,
                pending_probe: false,
                disable_state: "none".into(),
                disable_error: None,
                group_state: "none".into(),
                group_error: None,
            };
            // A deterministic key makes replay after an interrupted submission idempotent.
            let old = host::state_get(host, "jobs", &id).await?;
            if let Some(old) = old {
                let j: Job =
                    serde_json::from_value(old.value).map_err(|_| fault("已有提交记录损坏"))?;
                self.jobs.push((j, Some(old.version)));
            } else {
                self.jobs.push((job, None));
                let i = self.jobs.len() - 1;
                self.save_job(host, i).await?;
            }
            self.control.job_ids.push(id);
            self.save(host).await?;
        }
        Ok(submitted)
    }
    pub async fn cancel(
        &mut self,
        host: &HostClient,
        id: Option<&str>,
        source: Option<&str>,
    ) -> Result<(), PluginFault> {
        for i in 0..self.jobs.len() {
            if self.jobs[i].0.active()
                && id.is_none_or(|id| id == self.jobs[i].0.id)
                && source.is_none_or(|source| source == self.jobs[i].0.source)
            {
                self.jobs[i]
                    .0
                    .finish("cancelled", "inconclusive", "管理员取消检测", now());
                self.save_job(host, i).await?;
            }
        }
        Ok(())
    }
    pub async fn delete_job(&mut self, host: &HostClient, id: &str) -> Result<(), PluginFault> {
        let i = self
            .jobs
            .iter()
            .position(|(j, _)| j.id == id)
            .ok_or_else(|| fault("任务不存在"))?;
        if self.jobs[i].0.active() || self.jobs[i].0.actions_pending() {
            return Err(fault("请先取消活动检测，或等待调度与移组操作结束"));
        }
        self.control.job_ids.retain(|x| x != id);
        self.save(host).await?;
        host::state_delete(host, "jobs", id).await?;
        self.jobs.remove(i);
        self.publish(None);
        Ok(())
    }
    pub async fn delete_jobs(
        &mut self,
        host: &HostClient,
        ids: &[String],
    ) -> Result<Value, PluginFault> {
        let wanted: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
        let mut deleted = vec![];
        let mut skipped = 0;
        for (j, _) in &self.jobs {
            if wanted.contains(j.id.as_str()) {
                if j.active() || j.actions_pending() {
                    skipped += 1;
                } else {
                    deleted.push(j.id.clone());
                }
            }
        }
        if deleted.is_empty() {
            return Ok(json!({"deleted":0,"skipped":skipped}));
        }
        self.control.job_ids.retain(|id| !deleted.contains(id));
        self.save(host).await?;
        for id in &deleted {
            host::state_delete(host, "jobs", id).await?;
        }
        self.jobs.retain(|(j, _)| !deleted.contains(&j.id));
        self.publish(None);
        Ok(json!({"deleted":deleted.len(),"skipped":skipped}))
    }
    async fn prune(&mut self, host: &HostClient) -> Result<(), PluginFault> {
        // 历史固定保留最近三天，不设条数上限；存储预算仍是宿主的最终约束。
        let cutoff = now().saturating_sub(u64::from(HISTORY_RETENTION_DAYS) * 86400000);
        let mut bytes: usize = self
            .jobs
            .iter()
            .map(|(j, _)| serde_json::to_vec(j).unwrap().len())
            .sum();
        let mut removed = vec![];
        for (j, _) in &self.jobs {
            if !j.active()
                && !j.actions_pending()
                && (j.finished_at.unwrap_or(j.created_at) < cutoff || bytes > 12 * 1024 * 1024)
            {
                bytes = bytes.saturating_sub(serde_json::to_vec(j).unwrap().len());
                removed.push(j.id.clone());
            }
        }
        if !removed.is_empty() {
            self.control.job_ids.retain(|id| !removed.contains(id));
            self.save(host).await?;
            for id in &removed {
                host::state_delete(host, "jobs", id).await?;
            }
            self.jobs.retain(|(j, _)| !removed.contains(&j.id));
            self.publish(None);
        }
        Ok(())
    }
    async fn gc_banks(&mut self, host: &HostClient) -> Result<(), PluginFault> {
        let builtin = banks::builtin().hash;
        let mut keep = BTreeSet::from([builtin, self.control.active_bank.clone()]);
        if let Some(p) = &self.control.previous_bank {
            keep.insert(p.clone());
        }
        for (j, _) in &self.jobs {
            if j.active()
                && let Some(h) = &j.bank_hash
            {
                keep.insert(h.clone());
            }
        }
        // Delete data before metadata: cleanup is safe to retry while holding the controller lock.
        let obsolete: Vec<_> = self
            .control
            .banks
            .iter()
            .filter(|b| !keep.contains(&b.hash))
            .cloned()
            .collect();
        for b in obsolete {
            banks::remove(host, &b).await?;
            self.control.banks.retain(|v| v.hash != b.hash);
        }
        self.save(host).await
    }
    pub async fn check_update(&mut self, host: &HostClient) -> Result<(), PluginFault> {
        self.control.update.last_attempt_at = Some(now());
        self.save(host).await?;
        let mirror = self.control.config.mirror_prefix.clone();
        let result = tokio::time::timeout(std::time::Duration::from_secs(24), async {
            let sha = banks::latest(host, &mirror).await?;
            let data = banks::download(host, &mirror, &sha).await?;
            let bank = banks::validate(&mut self.engine, &data, sha).await?;
            Ok::<_, PluginFault>(bank)
        })
        .await
        .unwrap_or_else(|_| Err(fault("更新检查超过24秒，当前指纹库保持可用")));
        match result {
            Ok(bank) => {
                let current = self.active_bank();
                self.control.update.added = bank
                    .models
                    .iter()
                    .filter(|m| !current.models.iter().any(|n| n.id == m.id))
                    .map(|m| m.id.clone())
                    .collect();
                self.control.update.removed = current
                    .models
                    .iter()
                    .filter(|m| !bank.models.iter().any(|n| n.id == m.id))
                    .map(|m| m.id.clone())
                    .collect();
                self.control.update.available = if bank.hash != current.hash {
                    Some(bank)
                } else {
                    None
                };
                self.control.update.checked_at = Some(now());
                self.control.update.error = None;
            }
            Err(e) => {
                self.control.update.error = Some(e.message);
                self.control.update.available = None;
            }
        }
        self.save(host).await
    }
    pub async fn apply_update(&mut self, host: &HostClient) -> Result<(), PluginFault> {
        let candidate = self
            .control
            .update
            .available
            .clone()
            .ok_or_else(|| fault("请先检查更新，没有待应用版本"))?;
        self.gc_banks(host).await?;
        if !self.control.banks.iter().any(|b| b.hash == candidate.hash)
            && self.control.banks.iter().filter(|b| b.chunks > 0).count() >= 3
        {
            return Err(fault("旧任务仍引用指纹库，待任务完成后再应用更新"));
        }
        let mirror = self.control.config.mirror_prefix.clone();
        let data = tokio::time::timeout(
            std::time::Duration::from_secs(24),
            banks::download(host, &mirror, &candidate.commit),
        )
        .await
        .map_err(|_| fault("指纹库下载超时"))??;
        if banks::hash(&data) != candidate.hash {
            return Err(fault("下载内容与检查时摘要不一致，请重新检查"));
        }
        let mut bank = banks::validate(&mut self.engine, &data, candidate.commit).await?;
        self.control
            .config
            .validate(&bank.models.iter().map(|m| m.id.clone()).collect::<Vec<_>>())
            .map_err(fault)?;
        if !self.control.banks.iter().any(|b| b.hash == bank.hash) {
            let chunks = banks::encode(&data)?;
            bank.chunks = chunks.len();
            self.control.pending_bank = Some(bank.clone());
            self.save(host).await?;
            banks::persist(host, &bank, &chunks).await?;
            self.control.banks.push(bank.clone());
        }
        self.control.previous_bank = Some(self.control.active_bank.clone());
        self.control.active_bank = bank.hash;
        self.control.pending_bank = None;
        self.control.update.available = None;
        self.control.update.error = None;
        self.save(host).await?;
        self.gc_banks(host).await
    }
    pub async fn switch_bank(
        &mut self,
        host: &HostClient,
        builtin: bool,
    ) -> Result<(), PluginFault> {
        let target = if builtin {
            banks::builtin().hash
        } else {
            self.control
                .previous_bank
                .clone()
                .ok_or_else(|| fault("没有可回退的指纹库"))?
        };
        let bank = self.bank(&target)?;
        self.control
            .config
            .validate(&bank.models.iter().map(|m| m.id.clone()).collect::<Vec<_>>())
            .map_err(fault)?;
        let data = banks::read(host, &bank).await?;
        banks::validate(&mut self.engine, &data, bank.commit).await?;
        if target != self.control.active_bank {
            self.control.previous_bank = Some(self.control.active_bank.clone());
            self.control.active_bank = target;
            self.save(host).await?;
        }
        Ok(())
    }
    async fn check_identity(&self, host: &HostClient, i: usize) -> Result<Identity, PluginFault> {
        let j = &self.jobs[i].0;
        let a = host::runtime(host, &j.account_id).await?;
        if a.account_id != j.account_id || a.provider_id != "openai" || !a.enabled {
            return Err(fault("账户不存在、已停用或提供方已变化"));
        }
        let ident = host::identity(&a);
        if let Some(expected) = &j.identity
            && serde_json::to_value(expected).unwrap() != serde_json::to_value(&ident).unwrap()
        {
            return Err(fault("账户身份已变化，本次检测已终止"));
        }
        Ok(ident)
    }
    async fn validate_target_group(host: &HostClient, config: &Config) -> Result<(), PluginFault> {
        if config.auto_move_group
            && !host::groups(host)
                .await?
                .iter()
                .any(|g| g.id == config.target_group_id)
        {
            return Err(fault("降智目标分组不存在，请重新选择"));
        }
        Ok(())
    }
    // Disabled accounts remain eligible for moving: both actions can be enabled.
    async fn action_account(host: &HostClient, job: &Job) -> Result<bool, PluginFault> {
        let a = host::runtime(host, &job.account_id).await?;
        if a.account_id != job.account_id
            || a.provider_id != "openai"
            || job.identity.as_ref().is_none_or(|id| {
                serde_json::to_value(id).unwrap()
                    != serde_json::to_value(host::identity(&a)).unwrap()
            })
        {
            return Err(fault("账户身份已变化，调度操作已终止"));
        }
        Ok(a.enabled)
    }
    pub async fn disable(&mut self, host: &HostClient, i: usize) -> Result<(), PluginFault> {
        self.disable_before(host, i, None).await
    }
    async fn disable_before(
        &mut self,
        host: &HostClient,
        i: usize,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<(), PluginFault> {
        if self.jobs[i].0.verdict != "degraded" {
            return Err(fault("只有已确认降智的任务可执行停用"));
        }
        self.jobs[i].0.disable_state = "attempting".into();
        self.jobs[i].0.disable_error = None;
        self.jobs[i].0.action_message();
        self.save_job(host, i).await?;
        let action = async {
            let j = &self.jobs[i].0;
            if Self::action_account(host, j).await? {
                host::admin(
                    host,
                    "POST",
                    "/api/admin/accounts/batch-update",
                    Some(json!({"accountIds":[j.account_id],"enabled":false})),
                )
                .await?;
            }
            if Self::action_account(host, j).await? {
                return Err(fault("账户关闭调度后校验未通过"));
            }
            Ok(())
        };
        let result = match deadline {
            Some(deadline) => match tokio::time::timeout_at(deadline, action).await {
                Ok(result) => result,
                Err(_) => {
                    // Disabling is idempotent; defer to the next callback
                    // instead of surfacing a false failure when the budget
                    // was already exhausted.
                    self.jobs[i].0.disable_state = "pending".into();
                    self.jobs[i].0.disable_error = None;
                    self.jobs[i].0.action_message();
                    return self.save_job(host, i).await;
                }
            },
            None => action.await,
        };
        match result {
            Ok(()) => self.jobs[i].0.disable_state = "disabled".into(),
            Err(e) => {
                self.jobs[i].0.disable_state = "failed".into();
                self.jobs[i].0.disable_error = Some(e.message);
            }
        }
        self.jobs[i].0.action_message();
        self.save_job(host, i).await
    }
    pub async fn move_group(&mut self, host: &HostClient, i: usize) -> Result<(), PluginFault> {
        self.move_group_before(host, i, None).await
    }
    async fn move_group_before(
        &mut self,
        host: &HostClient,
        i: usize,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<(), PluginFault> {
        if self.jobs[i].0.verdict != "degraded" || !self.jobs[i].0.config.auto_move_group {
            return Err(fault("只有已确认降智且开启移组的任务可执行移组"));
        }
        self.jobs[i].0.group_state = "attempting".into();
        self.jobs[i].0.group_error = None;
        self.jobs[i].0.action_message();
        self.save_job(host, i).await?;
        let action = async {
            let j = &self.jobs[i].0;
            let target = vec![j.config.target_group_id.clone()];
            // Re-detection of an account that already sits in the target group
            // is a no-op. Check membership first so a late callback does not
            // spend its remaining budget on redundant identity/update calls.
            if host::account_groups(host, &j.account_id).await? == target {
                return Ok(());
            }
            Self::validate_target_group(host, &j.config).await?;
            Self::action_account(host, j).await?;
            // Replace memberships without changing enabled or other account settings.
            host::admin(
                host,
                "POST",
                "/api/admin/accounts/batch-update",
                Some(json!({"accountIds":[j.account_id],"groupIds":target})),
            )
            .await?;
            Self::action_account(host, j).await?;
            if host::account_groups(host, &j.account_id).await? != target {
                return Err(fault("账户移组后校验未通过"));
            }
            Ok(())
        };
        let result = match deadline {
            Some(deadline) => match tokio::time::timeout_at(deadline, action).await {
                Ok(result) => result,
                Err(_) => {
                    // The update is idempotent and membership is re-read on
                    // retry. Keep the action pending rather than reporting a
                    // failure when this callback simply ran out of budget.
                    self.jobs[i].0.group_state = "pending".into();
                    self.jobs[i].0.group_error = None;
                    self.jobs[i].0.action_message();
                    return self.save_job(host, i).await;
                }
            },
            None => action.await,
        };
        match result {
            Ok(()) => self.jobs[i].0.group_state = "moved".into(),
            Err(e) => {
                self.jobs[i].0.group_state = "failed".into();
                self.jobs[i].0.group_error = Some(e.message);
            }
        }
        self.jobs[i].0.action_message();
        self.save_job(host, i).await
    }
    pub async fn tick(
        &mut self,
        host: &HostClient,
        budget: std::time::Duration,
    ) -> Result<(), PluginFault> {
        // Up to 29s for requests; the warm worker can validate and score within
        // the final second. Keep a separate margin for state persistence.
        self.request_window = budget.saturating_sub(std::time::Duration::from_secs(1));
        self.deadline = Some(
            tokio::time::Instant::now()
                + budget.saturating_sub(std::time::Duration::from_millis(400)),
        );
        self.load(host).await?;
        self.recover_interrupted(host).await?;
        self.prune(host).await?;
        for i in 0..self.jobs.len() {
            if self.jobs[i].0.active() && self.progress.is_cancelled(&self.jobs[i].0.id) {
                self.jobs[i]
                    .0
                    .finish("cancelled", "inconclusive", "管理员取消检测", now());
                self.save_job(host, i).await?;
            }
        }
        if !self.jobs.iter().any(|(j, _)| j.actions_pending())
            && self.control.config.schedule.enabled
            && self.control.next_due.is_some_and(|t| t <= now())
        {
            let due = self.control.next_due.unwrap();
            let request = format!("schedule-{due}");
            self.control.pending_schedule = Some(request.clone());
            self.save(host).await?;
            let rows = host::accounts(host).await?;
            let s = &self.control.config.schedule;
            let ids =
                rows.iter()
                    .filter(|a| {
                        a.enabled
                            && a.supported
                            && if s.scope == "ids" {
                                s.account_ids.contains(&a.id)
                            } else {
                                a.matches(&s.filter)
                            }
                    })
                    .filter(|a| {
                        !self.jobs.iter().any(|(j, _)| {
                            j.active() && j.source == "schedule" && j.account_id == a.id
                        })
                    })
                    .map(|a| a.id.clone())
                    .collect::<Vec<_>>();
            let result = if ids.is_empty() {
                Ok(vec![])
            } else {
                self.submit(host, ids, &request, "schedule").await
            };
            self.control.schedule_error = result.err().map(|e| e.message);
            self.control.last_schedule_at = Some(now());
            self.control.next_due = Some(next_due(&self.control.config.schedule, now()));
            self.control.pending_schedule = None;
            return self.save(host).await;
        }
        let mut visited = BTreeSet::new();
        let mut continuing = false;
        loop {
            // Finish confirmed actions before dispatching another account. A
            // completed action is persisted before the next one starts.
            if let Some(i) = self
                .jobs
                .iter()
                .position(|(j, _)| j.disable_state == "pending")
            {
                if self.remaining() < std::time::Duration::from_secs(6) {
                    break;
                }
                self.disable_before(
                    host,
                    i,
                    self.deadline
                        .map(|d| d - std::time::Duration::from_millis(500)),
                )
                .await?;
                continuing = true;
                continue;
            }
            if let Some(i) = self
                .jobs
                .iter()
                .position(|(j, _)| j.group_state == "pending")
            {
                if self.remaining() < std::time::Duration::from_secs(6) {
                    break;
                }
                self.move_group_before(
                    host,
                    i,
                    self.deadline
                        .map(|d| d - std::time::Duration::from_millis(500)),
                )
                .await?;
                continuing = true;
                continue;
            }
            if self.remaining() < std::time::Duration::from_secs(6) {
                break;
            }
            let ready = self.ready_queue(&visited);
            let Some(head) = ready.first().copied() else {
                break;
            };
            // A validated sample only needs a fast scoring call before the
            // round can complete, so grade it before dispatching probes.
            if !self.jobs[head].0.samples.is_empty() {
                visited.insert(head);
                self.last_job_source = Some(self.jobs[head].0.source.clone());
                continuing = true;
                if let Err(e) = self.score_ready(host, head).await {
                    if !self.loaded {
                        return Err(e);
                    }
                    self.jobs[head]
                        .0
                        .finish("failed", "inconclusive", e.message, now());
                    self.save_job(host, head).await?;
                }
                continue;
            }
            // Probes of different accounts are independent: each request pins
            // its own account route, so one maintenance callback can advance
            // the first round of up to N accounts simultaneously.
            let wave: Vec<usize> = ready.into_iter().take(self.concurrency()).collect();
            let reserve = self.effective_reserve(Self::finish_reserve(wave.len()));
            let mut probes = JoinSet::new();
            let mut spawned = false;
            for i in &wave {
                visited.insert(*i);
                self.last_job_source = Some(self.jobs[*i].0.source.clone());
                match self.begin_probe(host, *i, continuing, reserve).await {
                    Ok(Some(task)) => {
                        spawned = true;
                        probes.spawn(task.run());
                    }
                    Ok(None) => {}
                    Err(e) => {
                        if !self.loaded {
                            return Err(e);
                        }
                        self.jobs[*i]
                            .0
                            .finish("failed", "inconclusive", e.message, now());
                        self.save_job(host, *i).await?;
                    }
                }
            }
            if !spawned {
                break;
            }
            let mut sampled = vec![];
            while let Some(completed) = probes.join_next().await {
                let (id, outcome) = completed.map_err(|_| fault("探针任务异常退出"))?;
                // Job records are stable during a callback; indices cannot be
                // invalidated by deletions or additions here.
                let index = self
                    .jobs
                    .iter()
                    .position(|(j, _)| j.id == id)
                    .ok_or_else(|| fault("探针任务返回了未知任务"))?;
                continuing = true;
                // Persist every probe outcome before grading any of them so
                // one slow score cannot strand the rest of a wave at the
                // maintenance deadline.
                match self.finish_probe(host, index, outcome).await {
                    Ok(true) => sampled.push(index),
                    Ok(false) => {}
                    Err(e) => {
                        if !self.loaded {
                            return Err(e);
                        }
                        self.jobs[index]
                            .0
                            .finish("failed", "inconclusive", e.message, now());
                        self.save_job(host, index).await?;
                    }
                }
            }
            for index in sampled {
                // The sample is already persisted at this point. If scoring
                // cannot finish before the deadline it records waiting_score
                // and the next callback retries without another model request.
                if let Err(e) = self.score_ready(host, index).await {
                    if !self.loaded {
                        return Err(e);
                    }
                    self.jobs[index]
                        .0
                        .finish("failed", "inconclusive", e.message, now());
                    self.save_job(host, index).await?;
                }
            }
        }
        // Background downloads must not occupy a callback ahead of ready probes
        // or account actions. Start them only on an idle callback.
        let busy = self
            .jobs
            .iter()
            .any(|(j, _)| j.actions_pending() || (j.active() && j.next_at <= now()));
        if visited.is_empty() && !busy && self.remaining() >= std::time::Duration::from_secs(6) {
            if self.remaining() >= std::time::Duration::from_secs(25)
                && self.control.config.auto_check_updates
                && self.control.update.last_attempt_at.is_none_or(|at| {
                    now().saturating_sub(at)
                        >= u64::from(self.control.config.update_interval_hours) * 3600000
                })
            {
                return self.check_update(host).await;
            }
            self.gc_banks(host).await?;
        }
        Ok(())
    }
    fn remaining(&self) -> std::time::Duration {
        self.deadline
            .map(|d| d.saturating_duration_since(tokio::time::Instant::now()))
            .unwrap_or_default()
    }
    fn concurrency(&self) -> usize {
        self.control.config.max_concurrency.max(1) as usize
    }
    /// Leave room at the end of the callback to persist every result in a
    /// parallel wave before the host cancels the maintenance RPC. Larger waves
    /// need more time for validation and per-job state writes.
    fn finish_reserve(wave: usize) -> std::time::Duration {
        let extra = u64::try_from(wave.saturating_sub(1)).unwrap_or(15) * 180;
        std::time::Duration::from_millis((300 + extra).min(3000))
    }
    fn effective_reserve(&self, requested: std::time::Duration) -> std::time::Duration {
        requested.min(
            self.remaining()
                .saturating_sub(std::time::Duration::from_secs(1)),
        )
    }
    fn ready_queue(&self, visited: &BTreeSet<usize>) -> Vec<usize> {
        // Alternate ready sources; each job gets at most one round or failed
        // attempt in this callback, even with a zero retry delay. Unstable
        // checks are folded into a numeric key so ties keep submission order.
        let mut queue: Vec<(usize, (u8, u8, u8))> = self
            .jobs
            .iter()
            .enumerate()
            .filter(|(i, (j, _))| {
                !visited.contains(i) && j.active() && !j.pending_probe && j.next_at <= now()
            })
            .map(|(i, (j, _))| {
                (
                    i,
                    (
                        u8::from(j.samples.is_empty()),
                        u8::from(self.last_job_source.as_deref() == Some(j.source.as_str())),
                        u8::from(j.status != "running"),
                    ),
                )
            })
            .collect();
        queue.sort_by_key(|(_, key)| *key);
        queue.into_iter().map(|(i, _)| i).collect()
    }
    /// Grades the stored sample of one job. Probes never run here: a job
    /// reaches this method only with a validated sample ready for scoring.
    async fn score_ready(&mut self, host: &HostClient, i: usize) -> Result<(), PluginFault> {
        if self.jobs[i].0.samples.len() != 1 {
            return Ok(());
        }
        // Scoring starts only when the callback still holds a safe share of
        // its budget; otherwise the validated sample waits for the next
        // dispatch instead of racing the callback deadline.
        if self.remaining() < std::time::Duration::from_millis(300) {
            self.progress
                .phase(&self.jobs[i].0.id, "waiting_score", json!({}));
            return Ok(());
        }
        self.progress
            .phase(&self.jobs[i].0.id, "scoring", json!({}));
        let deadline = self.deadline.unwrap() - std::time::Duration::from_millis(150);
        let scoring = async {
            let path = self.prepare_score(host, i).await?;
            self.engine
                .call::<_, Score>(
                    json!({"op":"score","path":path,"samples":self.jobs[i].0.samples}),
                )
                .await
        };
        let score = match tokio::time::timeout_at(deadline, scoring).await {
            Ok(result) => result?,
            Err(_) => {
                self.progress
                    .phase(&self.jobs[i].0.id, "waiting_score", json!({}));
                return Ok(()); // The validated sample is already persisted.
            }
        };
        let j = &mut self.jobs[i].0;
        let verdict = j.classify(&score);
        j.rounds.push(Round {
            number: j.rounds.len() as u32 + 1,
            verdict: verdict.into(),
            score,
            finished_at: now(),
        });
        j.samples.clear();
        j.challenges.clear();
        j.round_attempts = 0;
        match verdict {
            "degraded" => {
                j.finish("completed", "degraded", "已确认降智", now());
                if j.config.auto_disable {
                    j.disable_state = "pending".into();
                }
                if j.config.auto_move_group {
                    j.group_state = "pending".into();
                }
                j.action_message();
            }
            "suspected" | "low_confidence" => {
                let exhausted = j.rounds.len() as u32
                    >= j.config.confirmation_rounds + j.config.low_confidence_retries
                    || verdict == "low_confidence"
                        && j.low_retries_used >= j.config.low_confidence_retries;
                if exhausted {
                    j.finish(
                        "completed",
                        "inconclusive",
                        "复测预算已耗尽，证据不足，不关闭调度",
                        now(),
                    );
                } else {
                    if verdict == "low_confidence" {
                        j.low_retries_used += 1;
                    }
                    j.verdict = verdict.into();
                    j.message = if verdict == "low_confidence" {
                        "所有模型概率均低于阈值，等待自动复测"
                    } else {
                        "疑似降智，等待连续复测确认"
                    }
                    .into();
                    j.next_at = now() + u64::from(j.config.retry_delay_seconds) * 1000;
                }
            }
            "no_match" => j.finish("completed", "no_match", "未降智", now()),
            _ => j.finish(
                "completed",
                "inconclusive",
                "概率未达到降智判定阈值，证据不足",
                now(),
            ),
        }
        if j.active() {
            self.progress
                .phase(&j.id, "waiting_retest", json!({"nextAt":j.next_at}));
        }
        self.save_job(host, i).await
    }
    /// Prepares one account for a model probe and returns the request task.
    /// `Ok(None)` means nothing will be sent this callback: the request was
    /// cancelled, had been interrupted before, or must not start inside a
    /// shortened window. All bookkeeping up to `pending_probe` stays here so
    /// the spawned task only performs network work.
    async fn begin_probe(
        &mut self,
        host: &HostClient,
        i: usize,
        continuing: bool,
        reserve: std::time::Duration,
    ) -> Result<Option<ProbeTask>, PluginFault> {
        if self.progress.is_cancelled(&self.jobs[i].0.id) {
            self.jobs[i]
                .0
                .finish("cancelled", "inconclusive", "管理员取消检测", now());
            return self.save_job(host, i).await.map(|()| None);
        }
        if self.jobs[i].0.pending_probe {
            self.jobs[i].0.finish(
                "interrupted",
                "inconclusive",
                "上一次探针执行被中断，请新建检测",
                now(),
            );
            return self.save_job(host, i).await.map(|()| None);
        }
        if self.jobs[i].0.status == "queued" {
            let ident = self.check_identity(host, i).await?;
            let bank = self.active_bank();
            self.jobs[i].0.identity = Some(ident);
            self.jobs[i].0.bank_hash = Some(bank.hash);
            self.jobs[i].0.bank_commit = Some(bank.commit);
            self.jobs[i].0.status = "running".into();
            self.jobs[i].0.message = "准备第一轮挑战".into();
            self.save_job(host, i).await?;
        }
        if self.jobs[i].0.challenges.is_empty() {
            self.progress
                .phase(&self.jobs[i].0.id, "preparing", json!({}));
            self.jobs[i].0.challenges = self.engine.call(json!({"op":"challenges"})).await?;
            self.prepare_score(host, i).await?;
            self.save_job(host, i).await?;
        }
        // Cold extraction/preparation must not silently shorten the very
        // first probes of a callback. Yield without sending or consuming a
        // retry attempt when the remaining window cannot hold the request.
        if !continuing {
            let request_window = self.request_window.saturating_sub(reserve);
            let desired = if self.jobs[i].0.config.timeout_seconds == 0 {
                request_window
            } else {
                request_window.min(std::time::Duration::from_secs(
                    self.jobs[i].0.config.timeout_seconds,
                ))
            };
            let available = self
                .remaining()
                .saturating_sub(std::time::Duration::from_millis(600));
            if available < desired.saturating_sub(std::time::Duration::from_millis(500)) {
                self.jobs[i].0.message = "准备完成，等待下一次调度发起请求".into();
                self.progress
                    .phase(&self.jobs[i].0.id, "waiting_request", json!({}));
                self.save_job(host, i).await?;
                return Ok(None);
            }
        }
        let challenge = self.jobs[i].0.challenges[self.jobs[i].0.samples.len()].clone();
        self.check_identity(host, i).await?;
        self.jobs[i].0.pending_probe = true;
        self.jobs[i].0.round_attempts += 1;
        self.jobs[i].0.message = format!("第{}轮，正在检测", self.jobs[i].0.rounds.len() + 1);
        self.save_job(host, i).await?;
        self.progress.phase(&self.jobs[i].0.id,"requesting",json!({"round":self.jobs[i].0.rounds.len()+1,"attempt":self.jobs[i].0.round_attempts,"expectedCount":challenge.expected_count}));
        Ok(Some(ProbeTask {
            job: self.jobs[i].0.clone(),
            prompt: challenge.prompt,
            host: host.clone(),
            deadline: self.deadline.map(|d| d.checked_sub(reserve).unwrap_or(d)),
            progress: self.progress.clone(),
        }))
    }
    /// Consumes one probe outcome: records the attempt, validates a received
    /// answer and updates the job. Returns whether a validated sample now
    /// waits for scoring.
    async fn finish_probe(
        &mut self,
        host: &HostClient,
        i: usize,
        result: Result<ExecutionResult, Box<ExecutionError>>,
    ) -> Result<bool, PluginFault> {
        if self.progress.is_cancelled(&self.jobs[i].0.id) {
            let request_id = match result {
                Ok(result) => Some(result.request_id),
                Err(error) => error.request_id,
            };
            self.jobs[i].0.attempts.push(Attempt {
                request_id,
                at: now(),
                error: Some("管理员取消检测".into()),
                diagnostics: None,
            });
            self.jobs[i].0.pending_probe = false;
            self.jobs[i]
                .0
                .finish("cancelled", "inconclusive", "管理员取消检测", now());
            return self.save_job(host, i).await.map(|()| false);
        }
        self.progress
            .phase(&self.jobs[i].0.id, "validating", json!({}));
        let mut permanent = false;
        let diagnostics;
        let (request_id, numbers, error) = match result {
            Ok(result) => {
                let challenge = &self.jobs[i].0.challenges[self.jobs[i].0.samples.len()];
                let partial = result.diagnostics.partial;
                diagnostics = Some(result.diagnostics);
                match self.engine.call::<_, Vec<u32>>(
                    json!({"op":"sample","text":result.text,"expectedCount":challenge.expected_count}),
                ).await {
                    Ok(n) => (Some(result.request_id), Some(n), None),
                    Err(e) => (Some(result.request_id), None, Some(if partial {
                        format!("部分结果未通过样本校验：{}", e.message)
                    } else { e.message })),
                }
            }
            Err(e) => {
                permanent = e.permanent;
                diagnostics = Some(e.diagnostics);
                (e.request_id, None, Some(e.message))
            }
        };
        let full_window_ms = self.request_window.as_millis().saturating_sub(500);
        let full_budget_timeout = |d: &ProbeDiagnostics| {
            d.kind == "host_budget_timeout"
                && d.budget_ms
                    .is_some_and(|ms| u128::from(ms) >= full_window_ms)
        };
        let challenge_expected =
            self.jobs[i].0.challenges[self.jobs[i].0.samples.len()].expected_count;
        let j = &mut self.jobs[i].0;
        let repeated_budget_timeout = diagnostics.as_ref().is_some_and(full_budget_timeout)
            && j.attempts
                .last()
                .filter(|a| a.error.is_some())
                .and_then(|a| a.diagnostics.as_ref())
                .is_some_and(full_budget_timeout);
        j.pending_probe = false;
        j.attempts.push(Attempt {
            request_id,
            at: now(),
            error: error.clone(),
            diagnostics,
        });
        let sampled;
        if let Some(numbers) = numbers {
            j.samples.push(Sample {
                numbers,
                expected_count: challenge_expected,
            });
            j.message = if j
                .attempts
                .last()
                .and_then(|a| a.diagnostics.as_ref())
                .is_some_and(|d| d.partial)
            {
                "已接纳本轮部分结果，样本校验通过，等待评分"
            } else {
                "已获得本轮有效样本，等待评分"
            }
            .into();
            sampled = true;
        } else if permanent || repeated_budget_timeout || j.round_attempts >= 5 {
            j.finish(
                "failed",
                "inconclusive",
                if repeated_budget_timeout {
                    format!(
                        "{}；连续两次超出宿主调用预算，已停止重复请求",
                        error.unwrap_or_default()
                    )
                } else {
                    error.unwrap_or_else(|| "采样失败".into())
                },
                now(),
            );
            sampled = false;
        } else {
            j.message = format!("{}；等待重试", error.unwrap_or_default());
            j.next_at = now() + u64::from(j.config.retry_delay_seconds) * 1000;
            self.progress
                .phase(&j.id, "waiting_retry", json!({"nextAt":j.next_at}));
            sampled = false;
        }
        self.save_job(host, i).await?;
        Ok(sampled)
    }
}

/// One in-flight model probe. The task owns only network work: it never
/// touches jobs, host state or the scoring worker, so several of them can run
/// at the same time inside a single maintenance callback.
struct ProbeTask {
    job: crate::domain::Job,
    prompt: String,
    host: HostClient,
    deadline: Option<tokio::time::Instant>,
    progress: Arc<Progress>,
}
impl ProbeTask {
    async fn run(self) -> (String, Result<ExecutionResult, Box<ExecutionError>>) {
        let outcome = host::execute(
            &self.host,
            &self.job,
            &self.prompt,
            self.deadline,
            self.progress,
        )
        .await;
        (self.job.id.clone(), outcome)
    }
}
