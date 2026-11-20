//! Native lm-detector scoring, vendored from the upstream Rust SDK (`sdk/rust`, lmfpd 0.2.0) of
//! https://github.com/Ikaleio/lm-detector. `analysis`, `challenge`, `codec`, `features` and `ranker` are upstream
//! files with only their module paths changed; `data` keeps upstream decoding without its network and cache layer.
//! Only the request/stream code is left out: the plugin sends probes through the CPR host.
#![allow(dead_code, unused_imports)]

#[rustfmt::skip]
mod analysis;
#[rustfmt::skip]
mod challenge;
#[rustfmt::skip]
mod codec;
#[rustfmt::skip]
mod data;
#[rustfmt::skip]
mod error;
#[rustfmt::skip]
mod features;
#[rustfmt::skip]
mod ranker;

use serde::{Deserialize, Serialize};

pub use challenge::generate_challenges;
pub use data::{DATA_SCHEMA, DETECTOR_SCHEMA, Model};
pub use error::Error;
pub use features::parse_numbers;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    pub text: String,
    pub expected_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Challenge {
    pub id: String,
    pub expected_count: u64,
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Scored,
    Partial,
    Unscorable,
    NotModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbabilityStatus {
    Unavailable,
    ReferenceCalibrated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub model: String,
    pub display_name: String,
    pub family: String,
    pub family_name: String,
    pub score: f64,
    pub probability: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    pub method: String,
    pub run: String,
    pub tau: f64,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub index: usize,
    pub parsed_numbers: usize,
    pub minimum_numbers: u64,
    pub accepted: bool,
    pub raw_out_of_range_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sequence {
    pub autocorrelation: f64,
    pub small_step_share: f64,
    pub model_like: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelVersion {
    pub base_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub insufficient: bool,
    pub label: String,
    pub reason: String,
    /// Always `None`; kept for the shared analysis shape.
    pub threshold: Option<f64>,
    pub method: String,
}

/// The scoring result. Every field is always present, so the shape matches the TS, Go, and Python SDKs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub decision: Decision,
    pub prediction: String,
    pub prediction_name: String,
    pub family_prediction: String,
    pub family_prediction_name: String,
    /// Candidates by ranking score, best first. Empty unless the decision is `Scored` or `Partial`.
    pub results: Vec<AnalysisResult>,
    pub ranking_score: Option<f64>,
    /// Closed-set probability of the top candidate within the reference bank.
    pub probability: Option<f64>,
    pub probability_status: ProbabilityStatus,
    pub probability_scope: Option<String>,
    pub calibration: Option<Calibration>,
    pub used_outputs: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub sequence: Option<Sequence>,
    pub method: String,
    pub model_version: ModelVersion,
    pub evidence: Evidence,
}

/// A decoded bank and detector. Built from the packed layout of the upstream cache file: the u32 little-endian
/// length of `bundle.json`, `bundle.json`, then `shared_detector.bin`.
pub struct Detector {
    data: data::ReferenceData,
}

impl Detector {
    pub fn decode(packed: &[u8]) -> Result<Self, Error> {
        let (bundle, detector) =
            unpack(packed).ok_or_else(|| Error::new("Cannot read the packed reference data."))?;
        Ok(Self {
            data: data::decode_data(bundle, detector, "bundle.json", "shared_detector.bin")?,
        })
    }

    pub fn built_at(&self) -> &str {
        &self.data.built_at
    }

    pub fn models(&self) -> &[Model] {
        &self.data.models
    }

    pub fn base_sha256(&self) -> &str {
        &self.data.base_sha256
    }

    pub fn calibrated(&self) -> bool {
        self.data.calibration.is_some()
    }

    /// Scores replies whose numbers were already parsed and capped at their expected count, like the upstream
    /// `Detector::detect` round scoring in relaxed mode: three valid replies give a calibrated ranking, fewer give a
    /// partial ranking without confidence.
    pub fn score(&self, samples: &[(Vec<u32>, u64)]) -> Result<Analysis, Error> {
        let texts: Vec<(String, u64)> = samples
            .iter()
            .map(|(n, e)| (analysis::join(n), *e))
            .collect();
        let outputs: Vec<(&str, u64)> = texts.iter().map(|(t, e)| (t.as_str(), *e)).collect();
        analysis::score(&outputs, &self.data, false)
    }

    /// Ranking scores straight from the ranker, used only to self-check a downloaded detector.
    pub fn rank(&self, numbers: &[Vec<u32>]) -> Option<Vec<f64>> {
        self.data.ranker.rank(numbers)
    }

    pub fn calibrate(&self, ranking: &[f64]) -> Option<Vec<f64>> {
        self.data
            .calibration
            .as_ref()
            .map(|(_, tau)| ranker::calibrate(ranking, *tau))
    }
}

/// The minimum valid numbers for a challenge, as upstream.
pub fn minimum_numbers(expected: u64) -> u64 {
    features::minimum_numbers(expected)
}

pub fn pack(bundle: &[u8], detector: &[u8]) -> Vec<u8> {
    let mut packed = Vec::with_capacity(4 + bundle.len() + detector.len());
    packed.extend_from_slice(&(bundle.len() as u32).to_le_bytes());
    packed.extend_from_slice(bundle);
    packed.extend_from_slice(detector);
    packed
}

pub fn unpack(packed: &[u8]) -> Option<(&[u8], &[u8])> {
    let length = u32::from_le_bytes(packed.get(..4)?.try_into().ok()?) as usize;
    let bundle = packed.get(4..4usize.checked_add(length)?)?;
    Some((bundle, &packed[4 + length..]))
}
