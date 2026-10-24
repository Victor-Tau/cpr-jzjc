use crate::host::now;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Mutex, RwLock},
};
use tokio::sync::Notify;

/// Read-only UI snapshots never wait for model execution or host storage.
pub struct Progress {
    state: RwLock<Snapshot>,
    cancelled: Mutex<BTreeSet<String>>,
    cancellation: Notify,
}

#[derive(Default)]
struct Snapshot {
    ready: bool,
    epoch: String,
    revision: u64,
    jobs: Vec<Value>,
    reports: BTreeMap<String, Value>,
    phases: BTreeMap<String, Value>,
    banks: Value,
    next_due: Option<u64>,
    schedule_error: Option<String>,
}

impl Progress {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(Snapshot {
                epoch: uuid::Uuid::new_v4().to_string(),
                ..Snapshot::default()
            }),
            cancelled: Mutex::new(BTreeSet::new()),
            cancellation: Notify::new(),
        }
    }

    pub fn publish(
        &self,
        jobs: Vec<Value>,
        banks: Value,
        next_due: Option<u64>,
        schedule_error: Option<String>,
        reports: impl Iterator<Item = (String, Value)>,
    ) {
        let mut s = self.state.write().unwrap();
        s.jobs = jobs;
        s.banks = banks;
        s.next_due = next_due;
        s.schedule_error = schedule_error;
        let ids: BTreeSet<_> = s
            .jobs
            .iter()
            .filter_map(|j| j["id"].as_str().map(str::to_owned))
            .collect();
        s.reports.retain(|id, _| ids.contains(id));
        s.phases.retain(|id, _| ids.contains(id));
        for (id, mut report) in reports {
            report.as_object_mut().unwrap().remove("samples");
            report.as_object_mut().unwrap().remove("challenges");
            report.as_object_mut().unwrap().remove("pendingOutcome");
            if !["queued", "running"].contains(&report["status"].as_str().unwrap_or("")) {
                s.phases.remove(&id);
                self.cancelled.lock().unwrap().remove(&id);
            }
            s.reports.insert(id, report);
        }
        s.ready = true;
        s.revision += 1;
    }

    pub fn phase(&self, id: &str, stage: &str, detail: Value) {
        let mut s = self.state.write().unwrap();
        let started = s
            .phases
            .get(id)
            .filter(|p| p["stage"] == stage)
            .map(|p| p["stageStartedAt"].clone())
            .unwrap_or_else(|| json!(now()));
        s.phases.insert(
            id.to_owned(),
            json!({"stage":stage,"stageStartedAt":started,"updatedAt":now(),"detail":detail}),
        );
        s.revision += 1;
    }

    fn jobs(s: &Snapshot) -> Vec<Value> {
        s.jobs
            .iter()
            .map(|job| {
                let mut job = job.clone();
                if let Some(p) = job["id"].as_str().and_then(|id| s.phases.get(id)) {
                    job["progress"] = p.clone();
                }
                job
            })
            .collect()
    }

    pub fn read(&self, since: Option<&str>, lightweight: bool) -> Option<Value> {
        let s = self.state.read().unwrap();
        if !s.ready {
            return None;
        }
        let revision = format!("{}:{}", s.epoch, s.revision);
        let changed = since != Some(revision.as_str());
        let mut v = json!({"revision":revision,"changed":changed,"jobs":if changed {Some(Self::jobs(&s))}else{None},"nextDue":s.next_due,"scheduleError":s.schedule_error});
        if !lightweight {
            v["banks"] = s.banks.clone();
        }
        Some(v)
    }

    pub fn report(&self, id: &str) -> Option<Value> {
        let s = self.state.read().unwrap();
        let mut r = s.reports.get(id)?.clone();
        if let Some(p) = s.phases.get(id) {
            r["progress"] = p.clone();
        }
        Some(r)
    }

    pub fn cancel(
        &self,
        id: Option<&str>,
        source: Option<&str>,
    ) -> Result<(), gateway_plugin_sdk::PluginFault> {
        let ids = {
            let s = self.state.read().unwrap();
            if !s.ready {
                return Err(crate::host::fault("插件尚未完成初始化"));
            }
            s.jobs
                .iter()
                .filter(|j| {
                    id.is_none_or(|id| j["id"] == id)
                        && source.is_none_or(|source| j["source"] == source)
                        && ["queued", "running"].contains(&j["status"].as_str().unwrap_or(""))
                })
                .filter_map(|j| j["id"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        };
        self.cancelled.lock().unwrap().extend(ids.iter().cloned());
        for id in ids {
            self.phase(&id, "cancelling", json!({}));
        }
        self.cancellation.notify_one();
        Ok(())
    }

    pub fn is_cancelled(&self, id: &str) -> bool {
        self.cancelled.lock().unwrap().contains(id)
    }

    pub async fn cancelled(&self, id: &str) {
        loop {
            let notified = self.cancellation.notified();
            if self.is_cancelled(id) {
                return;
            }
            notified.await;
        }
    }
}
