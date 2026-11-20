//! In-process lm-detector scoring. Replaces the former Python/NumPy worker: the detector is decoded once per
//! fingerprint bank and every call runs natively inside the plugin process.
use crate::domain::{Candidate, Challenge, Family, Sample, Score};
use crate::host::fault;
use crate::lmfpd::{self, Decision, Detector};
use gateway_plugin_sdk::PluginFault;
use std::{collections::BTreeMap, sync::Arc};

/// Largest reply accepted for parsing, as before.
const MAX_TEXT: usize = 1024 * 1024;

#[derive(Default)]
pub struct Engine {
    cache: Option<(String, Arc<Detector>)>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cleanup(&mut self) -> Result<(), PluginFault> {
        self.cache = None;
        Ok(())
    }

    /// Decodes a packed bank (cached by its SHA-256).
    pub fn load(&mut self, hash: &str, packed: &[u8]) -> Result<Arc<Detector>, PluginFault> {
        if let Some((cached, detector)) = &self.cache
            && cached == hash
        {
            return Ok(detector.clone());
        }
        let detector =
            Arc::new(Detector::decode(packed).map_err(|e| fault(format!("指纹库解码失败：{e}")))?);
        self.cache = Some((hash.into(), detector.clone()));
        Ok(detector)
    }

    pub fn cached(&self, hash: &str) -> Option<Arc<Detector>> {
        self.cache
            .as_ref()
            .filter(|(cached, _)| cached == hash)
            .map(|(_, detector)| detector.clone())
    }
}

/// Upstream challenges: random prompts asking for 292–332 distinct-length answers.
pub fn challenges(count: u32) -> Result<Vec<Challenge>, PluginFault> {
    lmfpd::generate_challenges(count as usize)
        .map_err(|e| fault(format!("挑战生成失败：{e}")))
        .map(|list| {
            list.into_iter()
                .map(|c| Challenge {
                    id: c.id,
                    prompt: c.prompt,
                    expected_count: c.expected_count as u32,
                })
                .collect()
        })
}

/// Accepts one reply like upstream relaxed sampling: numbers beyond the requested count are cut, fewer than
/// `max(80, ceil(0.55 × count))` are rejected. A partial (interrupted) reply drops a trailing digit run that may
/// belong to an unfinished number.
pub fn sample(text: &str, expected_count: u32, partial: bool) -> Result<Vec<u32>, PluginFault> {
    if text.len() > MAX_TEXT {
        return Err(fault("回答过大"));
    }
    let text = if partial {
        text.trim_end_matches(|c: char| c.is_ascii_digit())
    } else {
        text
    };
    let mut numbers = lmfpd::parse_numbers(text);
    numbers.truncate(expected_count as usize);
    let minimum = lmfpd::minimum_numbers(u64::from(expected_count));
    if (numbers.len() as u64) < minimum {
        return Err(fault(format!(
            "有效数字不足（{}，至少{minimum}）",
            numbers.len()
        )));
    }
    Ok(numbers)
}

/// Scores one round of accepted samples with the upstream round scoring.
pub fn score(detector: &Detector, samples: &[Sample]) -> Result<Score, PluginFault> {
    if !(1..=3).contains(&samples.len()) {
        return Err(fault("每轮评分需要1至3个有效样本"));
    }
    let input: Vec<(Vec<u32>, u64)> = samples
        .iter()
        .map(|s| (s.numbers.clone(), u64::from(s.expected_count)))
        .collect();
    let analysis = detector
        .score(&input)
        .map_err(|e| fault(format!("评分失败：{e}")))?;
    let results: Vec<Candidate> = analysis
        .results
        .iter()
        .map(|r| Candidate {
            model: r.model.clone(),
            label: r.display_name.clone(),
            family: r.family.clone(),
            probability: r.probability,
            score: Some(r.score),
        })
        .collect();
    // Family totals are only a display aggregate of the calibrated candidate probabilities.
    let mut families: Vec<Family> = vec![];
    if analysis.probability.is_some() {
        let mut totals: BTreeMap<&str, (String, f64)> = BTreeMap::new();
        for r in &analysis.results {
            let entry = totals
                .entry(r.family.as_str())
                .or_insert_with(|| (r.family_name.clone(), 0.0));
            entry.1 += r.probability.unwrap_or(0.0);
        }
        families = totals
            .into_iter()
            .map(|(family, (name, probability))| Family {
                family: family.into(),
                display_name: name,
                probability,
            })
            .collect();
        families.sort_by(|a, b| b.probability.total_cmp(&a.probability));
    }
    let decision = match analysis.decision {
        Decision::Scored => "scored",
        Decision::Partial => "partial",
        Decision::Unscorable => "unscorable",
        Decision::NotModel => "not_model",
    };
    Ok(Score {
        prediction: analysis.prediction,
        probability: analysis.probability,
        results,
        families: serde_json::to_value(families).unwrap(),
        decision: Some(decision.into()),
        ranking_score: analysis.ranking_score,
        used_outputs: Some(analysis.used_outputs as u32),
        method: Some(analysis.method),
        reason: Some(analysis.evidence.reason),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> Detector {
        Detector::decode(crate::banks::builtin_bytes()).unwrap()
    }

    #[test]
    fn builtin_bank_decodes_with_calibration() {
        let d = detector();
        assert!(d.calibrated());
        assert!(d.models().len() >= 2);
    }

    #[test]
    fn sample_caps_and_rejects_like_upstream() {
        let text = (0..320)
            .map(|i| ((i * 37 + 11) % 355 + 1).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        assert_eq!(sample(&text, 300, false).unwrap().len(), 300);
        assert!(sample("1, 2, 3", 300, false).is_err());
        // A partial reply drops the unfinished trailing run.
        let partial = format!("{text}, 3");
        assert_eq!(sample(&partial, 330, true).unwrap().len(), 320);
    }

    #[test]
    fn upstream_conformance_scores_match() {
        // Cases from upstream sdk/conformance/analyze.json evaluated against the bundled data must
        // at least produce a finite ranking for one and three samples.
        let d = detector();
        let numbers: Vec<u32> = [
            47, 203, 12, 289, 156, 84, 231, 19, 312, 105, 248, 73, 167, 326, 41, 198, 82, 264, 137,
            355, 29, 214, 91, 172, 303, 118, 256, 68, 185, 339, 34, 227, 142, 301, 77, 210, 163,
            49, 274, 120, 8, 293, 151, 236, 63, 318, 97, 181, 245, 25, 132, 284, 59, 207, 344, 113,
            176, 38, 262, 88, 221, 147, 309, 71, 195, 252, 16, 128, 280, 102, 239, 55, 333, 160,
            217, 93, 271, 44, 189, 124, 297, 66, 234, 109, 348, 179, 22, 258, 140, 315, 86, 201,
        ]
        .into_iter()
        .collect();
        let s = Sample {
            numbers,
            expected_count: 150,
        };
        let one = score(&d, std::slice::from_ref(&s)).unwrap();
        assert_eq!(one.decision.as_deref(), Some("partial"));
        assert!(one.probability.is_none());
        assert!(!one.results.is_empty());
        let three = score(&d, &[s.clone(), s.clone(), s]).unwrap();
        assert_eq!(three.decision.as_deref(), Some("scored"));
        let p = three.probability.unwrap();
        assert!((0.0..=1.0).contains(&p));
        let total: f64 = three.results.iter().map(|r| r.probability.unwrap()).sum();
        assert!((total - 1.0).abs() < 1e-9);
    }
}
