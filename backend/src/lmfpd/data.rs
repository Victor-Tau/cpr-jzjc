//! Reference data decoding (upstream `sdk/rust/src/data.rs`, without the network and cache layer: the plugin fetches
//! and stores the data through the host).

use serde::Deserialize;
use serde_json::Value;

use super::ranker::Ranker;
use super::{Calibration, Error, codec};

/// Bump when the bundle layout changes; older SDKs then refuse the data instead of misreading it.
pub const DATA_SCHEMA: &str = "lmfpd-data-v1";
pub const DETECTOR_SCHEMA: &str = "shared-detector-v2";
pub struct Model {
    pub id: String,
    pub display_name: String,
    pub family: String,
    pub family_name: String,
}

/// The decoded bank and detector, ready for scoring.
pub(crate) struct ReferenceData {
    pub built_at: String,
    pub models: Vec<Model>,
    pub base_sha256: String,
    pub ranker: Ranker,
    /// The calibration output and its temperature, present only when its binding matches this detector.
    pub calibration: Option<(Calibration, f64)>,
}

#[derive(Deserialize)]
struct Bank {
    #[serde(default)]
    built_at: Option<String>,
    #[serde(default)]
    models: Vec<BankModel>,
}

#[derive(Deserialize)]
struct BankModel {
    id: String,
    display_name: String,
    family: String,
    family_name: String,
    response_count: f64,
}

fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn strings(value: Option<&Value>) -> Option<Vec<&str>> {
    value?.as_array()?.iter().map(Value::as_str).collect()
}

/// The calibration head of `calibrateRanking`, checked once at load instead of on every call.
fn calibration(detector: &Value, model_ids: &[&str]) -> Option<(Calibration, f64)> {
    let head = detector.get("calibration")?;
    let binding = head.get("binding")?;
    let tau = head.get("tau")?.as_f64()?;
    let valid = string(head, "schema") == Some("shared-confidence-v2")
        && string(head, "method") == Some("ranking-temperature")
        && tau.is_finite()
        && (0.001..=1000.0).contains(&tau)
        && string(binding, "base_sha256").is_some_and(|s| Some(s) == string(detector, "base_sha256"))
        && string(binding, "reference_sha256").is_some_and(|s| Some(s) == string(detector, "source_reference_sha256"))
        && strings(binding.get("model_ids")).is_some_and(|ids| ids == model_ids);
    valid.then(|| {
        let output = Calibration {
            method: "ranking-temperature".into(),
            run: string(head, "calibration_run").unwrap_or_default().into(),
            tau,
            sha256: string(detector, "calibration_sha256").map(Into::into),
        };
        (output, tau)
    })
}

/// Decodes and cross-checks the bank and the detector. Scoring reads only `built_at` and the model names and counts,
/// which `bundle.json` repeats at its top level, so it can stand in for `unified_bank.json`. The names appear in read
/// errors.
pub(crate) fn decode_data(bank: &[u8], detector: &[u8], bank_name: &str, detector_name: &str) -> Result<ReferenceData, Error> {
    let bank: Bank = serde_json::from_slice(bank).map_err(|_| Error::new(format!("Cannot read {bank_name}.")))?;
    let cannot_read_detector = || Error::new(format!("Cannot read {detector_name}."));
    let decoded = codec::decode(detector).ok_or_else(cannot_read_detector)?;
    let header = &decoded.header;
    let model_ids = strings(header.get("model_ids")).unwrap_or_default();
    let response_counts = header.get("response_counts").and_then(|v| decoded.vec(v)).unwrap_or_default();
    let matches = string(header, "schema") == Some(DETECTOR_SCHEMA)
        && bank.built_at.is_some()
        && bank.built_at.as_deref() == string(header, "bank_built_at")
        && bank.models.len() == model_ids.len()
        && response_counts.len() == model_ids.len()
        && bank.models.iter().zip(&model_ids).zip(&response_counts).all(|((model, id), count)| {
            model.id == *id && model.response_count == *count
        });
    if !matches {
        return Err(Error::new("The reference bank does not match its detector."));
    }
    let ranker = decoded.ranker(model_ids.len()).ok_or_else(cannot_read_detector)?;
    let calibration = calibration(header, &model_ids);
    Ok(ReferenceData {
        built_at: bank.built_at.unwrap_or_default(),
        base_sha256: string(header, "base_sha256").ok_or_else(cannot_read_detector)?.into(),
        models: bank
            .models
            .into_iter()
            .map(|m| Model { id: m.id, display_name: m.display_name, family: m.family, family_name: m.family_name })
            .collect(),
        ranker,
        calibration,
    })
}

