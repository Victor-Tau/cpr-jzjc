use crate::progress::Progress;
use crate::{
    banks::{self, Bank, Update},
    domain::*,
    engine::Engine,
    host::{self, fault, now},
};
use gateway_plugin_sdk::{PluginFault, client::HostClient};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};

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
    pub engine: Engine,
    pub progress: Arc<Progress>,
    deadline: Option<tokio::time::Instant>,
    request_window: std::time::Duration,
    last_job_source: Option<String>,
}
impl Manager {
    pub fn new(progress: Arc<Progress>) -> Self {
        Self {
            control: Control::default(),
            version: None,
            jobs: vec![],
            loaded: false,
            recovered: false,
            engine: Engine::new(),
            progress,
            deadline: None,
            request_window: std::time::Duration::ZERO,
            last_job_source: None,
        }
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
        if self.loaded {
            return Ok(());
        }
        self.jobs.clear();
        if let Some(r) = host::state_get(host, "control", "main").await? {
            self.control =
                serde_json::from_value(r.value).map_err(|_| fault("插件状态格式错误"))?;
            self.version = Some(r.version);
        } else {
            self.control = Control::default();
            self.version = None;
        }
        for id in self.control.job_ids.clone() {
            if let Some(r) = host::state_get(host, "jobs", &id).await? {
                let mut job: Job =
                    serde_json::from_value(r.value).map_err(|_| fault("历史任务格式错误"))?;
                if job.active() && (!self.recovered || job.pending_probe) {
                    job.finish(
                        "interrupted",
                        "inconclusive",
                        "插件或宿主已重启或探针中断；请重新检测",
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
    pub async fn set_config(
        &mut self,
        host: &HostClient,
        config: Config,
        revision: u64,
    ) -> Result<(), PluginFault> {
        if revision != self.control.config_revision {
            return Err(fault("配置已由其他页面更新，请刷新后重试"));
        }
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
        let new_count = selected.iter().filter(|(_, a)| a.is_some()).count();
        if self.jobs.len() + new_count > 500 {
            return Err(fault(
                "任务数量达到500条上限，请删除部分历史或等待保留期清理",
            ));
        }
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
    async fn prune(&mut self, host: &HostClient) -> Result<(), PluginFault> {
        let cutoff = now().saturating_sub(u64::from(self.control.config.retention_days) * 86400000);
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
                if removed.len() >= 20 {
                    break;
                }
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
        if self.jobs[i].0.verdict != "degraded" {
            return Err(fault("只有已确认降智的任务可执行停用"));
        }
        self.jobs[i].0.disable_state = "attempting".into();
        self.jobs[i].0.disable_error = None;
        self.jobs[i].0.action_message();
        self.save_job(host, i).await?;
        let result = async {
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
        }
        .await;
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
        if self.jobs[i].0.verdict != "degraded" || !self.jobs[i].0.config.auto_move_group {
            return Err(fault("只有已确认降智且开启移组的任务可执行移组"));
        }
        self.jobs[i].0.group_state = "attempting".into();
        self.jobs[i].0.group_error = None;
        self.jobs[i].0.action_message();
        self.save_job(host, i).await?;
        let result = async {
            let j = &self.jobs[i].0;
            Self::validate_target_group(host, &j.config).await?;
            Self::action_account(host, j).await?;
            let target = vec![j.config.target_group_id.clone()];
            if host::account_groups(host, &j.account_id).await? != target {
                // Replace memberships without changing enabled or other account settings.
                Self::action_account(host, j).await?;
                host::admin(
                    host,
                    "POST",
                    "/api/admin/accounts/batch-update",
                    Some(json!({"accountIds":[j.account_id],"groupIds":target})),
                )
                .await?;
            }
            Self::action_account(host, j).await?;
            if host::account_groups(host, &j.account_id).await? != target {
                return Err(fault("账户移组后校验未通过"));
            }
            Ok(())
        }
        .await;
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
        // Give the network up to 28s of a 30s call. The worker is already warm
        // before dispatch; validation and persistence retain 2s, scoring can wait.
        self.request_window = budget.saturating_sub(std::time::Duration::from_secs(2));
        self.deadline = Some(
            tokio::time::Instant::now()
                + budget.saturating_sub(std::time::Duration::from_millis(1500)),
        );
        self.load(host).await?;
        self.prune(host).await?;
        for i in 0..self.jobs.len() {
            if self.jobs[i].0.active() && self.progress.is_cancelled(&self.jobs[i].0.id) {
                self.jobs[i]
                    .0
                    .finish("cancelled", "inconclusive", "管理员取消检测", now());
                self.save_job(host, i).await?;
            }
        }
        if let Some(i) = self
            .jobs
            .iter()
            .position(|(j, _)| j.disable_state == "pending")
        {
            return self.disable(host, i).await;
        }
        if let Some(i) = self
            .jobs
            .iter()
            .position(|(j, _)| j.group_state == "pending")
        {
            return self.move_group(host, i).await;
        }
        if self.control.config.schedule.enabled && self.control.next_due.is_some_and(|t| t <= now())
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
        if self.control.config.auto_check_updates
            && self.control.update.last_attempt_at.is_none_or(|at| {
                now().saturating_sub(at)
                    >= u64::from(self.control.config.update_interval_hours) * 3600000
            })
        {
            return self.check_update(host).await;
        }
        let next = self
            .jobs
            .iter()
            .enumerate()
            .filter(|(_, (j, _))| j.active() && j.next_at <= now())
            // Alternate ready sources so confirmation rounds from one source
            // cannot monopolize execution while the other has queued work.
            .min_by_key(|(_, (j, _))| {
                (
                    self.last_job_source.as_deref() == Some(j.source.as_str()),
                    j.status != "running",
                )
            })
            .map(|(i, _)| i);
        if let Some(i) = next
            && self.jobs[i].0.next_at <= now()
        {
            self.last_job_source = Some(self.jobs[i].0.source.clone());
            if let Err(e) = self.advance(host, i).await {
                if !self.loaded {
                    return Err(e);
                }
                self.jobs[i]
                    .0
                    .finish("failed", "inconclusive", e.message, now());
                self.save_job(host, i).await?;
            }
            return Ok(());
        }
        self.gc_banks(host).await
    }
    async fn advance(&mut self, host: &HostClient, i: usize) -> Result<(), PluginFault> {
        let round = self.jobs[i].0.rounds.len();
        for _ in 0..5 {
            if self.progress.is_cancelled(&self.jobs[i].0.id) {
                self.jobs[i]
                    .0
                    .finish("cancelled", "inconclusive", "管理员取消检测", now());
                return self.save_job(host, i).await;
            }
            let remaining = self
                .deadline
                .map(|d| d.saturating_duration_since(tokio::time::Instant::now()))
                .unwrap_or_default();
            if remaining < std::time::Duration::from_secs(6) {
                if self.jobs[i].0.active() && !self.jobs[i].0.samples.is_empty() {
                    // The sample is already validated and persisted; only
                    // scoring waits for the next maintenance call. Report the
                    // wait accurately instead of leaving the UI on "validating".
                    self.progress
                        .phase(&self.jobs[i].0.id, "waiting_score", json!({}));
                }
                break;
            }
            let job = &self.jobs[i].0;
            if !job.challenges.is_empty() && job.samples.is_empty() {
                let desired = if job.config.timeout_seconds == 0 {
                    self.request_window
                } else {
                    self.request_window
                        .min(std::time::Duration::from_secs(job.config.timeout_seconds))
                };
                let available = remaining.saturating_sub(std::time::Duration::from_millis(500));
                if available < desired.saturating_sub(std::time::Duration::from_millis(500)) {
                    // Cold extraction/preparation must not silently shorten every
                    // probe. Yield without sending or consuming a retry attempt.
                    self.jobs[i].0.message = "准备完成，等待下一次调度发起请求".into();
                    self.progress
                        .phase(&self.jobs[i].0.id, "waiting_request", json!({}));
                    self.save_job(host, i).await?;
                    break;
                }
            }
            let attempts = self.jobs[i].0.attempts.len();
            self.advance_step(host, i).await?;
            let job = &self.jobs[i].0;
            if !job.active()
                || job.rounds.len() > round
                || job.next_at > now()
                || (job.attempts.len() > attempts && job.samples.is_empty())
            {
                break;
            }
        }
        Ok(())
    }
    async fn advance_step(&mut self, host: &HostClient, i: usize) -> Result<(), PluginFault> {
        if self.jobs[i].0.pending_probe {
            self.jobs[i].0.finish(
                "interrupted",
                "inconclusive",
                "上一次探针执行被中断，请新建检测",
                now(),
            );
            return self.save_job(host, i).await;
        }
        let ident = self.check_identity(host, i).await?;
        if self.jobs[i].0.status == "queued" {
            let bank = self.active_bank();
            self.jobs[i].0.identity = Some(ident);
            self.jobs[i].0.bank_hash = Some(bank.hash);
            self.jobs[i].0.bank_commit = Some(bank.commit);
            self.jobs[i].0.status = "running".into();
            self.jobs[i].0.message = "准备第一轮挑战".into();
            return self.save_job(host, i).await;
        }
        // Each confirmation round uses one valid model response. Additional
        // requests are retries for failures or belong to later rounds.
        if self.jobs[i].0.samples.len() == 1 {
            self.progress
                .phase(&self.jobs[i].0.id, "scoring", json!({}));
            let bank = self.bank(self.jobs[i].0.bank_hash.as_ref().unwrap())?;
            let bytes = banks::read(host, &bank).await?;
            let path = self.engine.write_bank(&bank.hash, &bytes).await?;
            let score: Score = self
                .engine
                .call(json!({"op":"score","path":path,"samples":self.jobs[i].0.samples}))
                .await?;
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
                "no_match" => j.finish("completed", "no_match", "未命中配置的降智规则", now()),
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
            return self.save_job(host, i).await;
        }
        if self.jobs[i].0.challenges.is_empty() {
            self.progress
                .phase(&self.jobs[i].0.id, "preparing", json!({}));
            self.jobs[i].0.challenges = self.engine.call(json!({"op":"challenges"})).await?;
            self.save_job(host, i).await?;
            return Ok(());
        }
        let challenge = self.jobs[i].0.challenges[self.jobs[i].0.samples.len()].clone();
        self.jobs[i].0.pending_probe = true;
        self.jobs[i].0.round_attempts += 1;
        self.jobs[i].0.message = format!("第{}轮，正在检测", self.jobs[i].0.rounds.len() + 1);
        self.save_job(host, i).await?;
        self.progress.phase(&self.jobs[i].0.id,"requesting",json!({"round":self.jobs[i].0.rounds.len()+1,"attempt":self.jobs[i].0.round_attempts,"expectedCount":challenge.expected_count}));
        let request_deadline = self
            .deadline
            .map(|d| d - std::time::Duration::from_millis(500));
        let result = host::execute(
            host,
            &self.jobs[i].0,
            &challenge.prompt,
            request_deadline,
            self.progress.clone(),
        )
        .await;
        if self.progress.is_cancelled(&self.jobs[i].0.id) {
            let request_id = match result {
                Ok((id, _)) => Some(id),
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
            return self.save_job(host, i).await;
        }
        self.progress
            .phase(&self.jobs[i].0.id, "validating", json!({}));
        let mut permanent = false;
        let mut diagnostics = None;
        let (request_id, numbers, error) = match result {
            Ok((id, text)) => match self
                .engine
                .call::<_, Vec<u32>>(
                    json!({"op":"sample","text":text,"expectedCount":challenge.expected_count}),
                )
                .await
            {
                Ok(n) => (Some(id), Some(n), None),
                Err(e) => (Some(id), None, Some(e.message)),
            },
            Err(e) => {
                permanent = e.permanent;
                diagnostics = Some(e.diagnostics);
                (e.request_id, None, Some(e.message))
            }
        };
        let j = &mut self.jobs[i].0;
        let repeated_budget_timeout = diagnostics
            .as_ref()
            .is_some_and(|d| d.kind == "host_budget_timeout")
            && j.attempts
                .last()
                .and_then(|a| a.diagnostics.as_ref())
                .is_some_and(|d| d.kind == "host_budget_timeout");
        j.pending_probe = false;
        j.attempts.push(Attempt {
            request_id,
            at: now(),
            error: error.clone(),
            diagnostics,
        });
        if let Some(numbers) = numbers {
            j.samples.push(Sample {
                numbers,
                expected_count: challenge.expected_count,
            });
            j.message = "已获得本轮有效样本，等待评分".into();
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
        } else {
            j.message = format!("{}；等待重试", error.unwrap_or_default());
            j.next_at = now() + u64::from(j.config.retry_delay_seconds) * 1000;
            self.progress
                .phase(&j.id, "waiting_retry", json!({"nextAt":j.next_at}));
        }
        self.save_job(host, i).await
    }
}
