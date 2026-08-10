//! Turning router decisions into the report. The headline is the confusion
//! matrix BETWEEN skills — "angular-testing fired on 7/10 queries meant for
//! react-testing" — because cross-skill collisions are the dominant real
//! problem past ~10 skills and are invisible when you look at one skill alone.

use serde::Serialize;
use ts_rs::TS;

use super::testset::TriggerCase;

pub const NONE_LABEL: &str = "∅ none";

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteResult {
    pub case_id: u32,
    /// The single skill the model routed to, or None.
    pub routed: Option<String>,
    /// Other skills whose descriptions also plausibly matched (latent overlap).
    #[serde(default)]
    pub also_plausible: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct MatrixRow {
    pub intended: String,
    pub counts: Vec<u32>,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ConfusionMatrix {
    /// Column labels (routed-to): catalog skills, then any stray labels, NONE last.
    pub labels: Vec<String>,
    pub rows: Vec<MatrixRow>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SkillMetrics {
    pub name: String,
    pub intended_total: u32,
    pub tp: u32,
    pub fp: u32,
    #[serde(rename = "fn")]
    pub fn_: u32,
    pub precision: f64,
    pub recall: f64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Collision {
    pub intended: String,
    pub routed: String,
    pub count: u32,
    pub intended_total: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LatentCollision {
    pub skill: String,
    pub also: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct EvalReport {
    pub matrix: ConfusionMatrix,
    pub metrics: Vec<SkillMetrics>,
    pub collisions: Vec<Collision>,
    pub latent: Vec<LatentCollision>,
    pub total_cases: u32,
    pub correct: u32,
    pub accuracy: f64,
}

fn label_of(name: &Option<String>) -> String {
    name.clone().unwrap_or_else(|| NONE_LABEL.to_string())
}

pub fn build_report(
    catalog_names: &[String],
    cases: &[TriggerCase],
    results: &[RouteResult],
) -> EvalReport {
    use std::collections::HashMap;

    let routed_by_id: HashMap<u32, &RouteResult> =
        results.iter().map(|r| (r.case_id, r)).collect();

    // Column labels: catalog skills, then any stray routed labels, NONE last.
    let mut labels: Vec<String> = catalog_names.to_vec();
    for r in results {
        if let Some(name) = &r.routed {
            if !labels.contains(name) {
                labels.push(name.clone());
            }
        }
    }
    labels.push(NONE_LABEL.to_string());
    let col_index: HashMap<String, usize> =
        labels.iter().enumerate().map(|(i, l)| (l.clone(), i)).collect();

    // Row labels: catalog skills + NONE (intended can only be a skill or none).
    let mut row_labels: Vec<String> = catalog_names.to_vec();
    row_labels.push(NONE_LABEL.to_string());

    let mut counts: HashMap<String, Vec<u32>> = row_labels
        .iter()
        .map(|l| (l.clone(), vec![0u32; labels.len()]))
        .collect();

    let mut correct = 0u32;
    let mut total = 0u32;

    // Per-skill tallies.
    let mut tp: HashMap<String, u32> = HashMap::new();
    let mut fp: HashMap<String, u32> = HashMap::new();
    let mut fn_: HashMap<String, u32> = HashMap::new();
    let mut intended_total: HashMap<String, u32> = HashMap::new();
    let mut latent: HashMap<(String, String), u32> = HashMap::new();

    for case in cases {
        let Some(result) = routed_by_id.get(&case.id) else {
            continue;
        };
        total += 1;
        let intended = label_of(&case.intended);
        let routed = label_of(&result.routed);

        if let Some(row) = counts.get_mut(&intended) {
            if let Some(&c) = col_index.get(&routed) {
                row[c] += 1;
            }
        }
        if intended == routed {
            correct += 1;
        }

        // Per-skill precision/recall over catalog skills.
        for skill in catalog_names {
            let is_intended = &intended == skill;
            let is_routed = &routed == skill;
            if is_intended {
                *intended_total.entry(skill.clone()).or_default() += 1;
                if is_routed {
                    *tp.entry(skill.clone()).or_default() += 1;
                } else {
                    *fn_.entry(skill.clone()).or_default() += 1;
                }
            } else if is_routed {
                *fp.entry(skill.clone()).or_default() += 1;
            }
        }

        // Latent overlap: intended skill A, another skill B also plausible.
        if let Some(a) = &case.intended {
            for b in &result.also_plausible {
                if b != a && catalog_names.contains(b) {
                    *latent.entry((a.clone(), b.clone())).or_default() += 1;
                }
            }
        }
    }

    let matrix = ConfusionMatrix {
        labels: labels.clone(),
        rows: row_labels
            .iter()
            .map(|l| {
                let c = counts.get(l).cloned().unwrap_or_default();
                let total = c.iter().sum();
                MatrixRow {
                    intended: l.clone(),
                    counts: c,
                    total,
                }
            })
            .collect(),
    };

    let mut metrics: Vec<SkillMetrics> = catalog_names
        .iter()
        .map(|name| {
            let tp = *tp.get(name).unwrap_or(&0);
            let fp = *fp.get(name).unwrap_or(&0);
            let fn_ = *fn_.get(name).unwrap_or(&0);
            let precision = ratio(tp, tp + fp);
            let recall = ratio(tp, tp + fn_);
            SkillMetrics {
                name: name.clone(),
                intended_total: *intended_total.get(name).unwrap_or(&0),
                tp,
                fp,
                fn_,
                precision,
                recall,
            }
        })
        .collect();
    metrics.sort_by(|a, b| a.recall.partial_cmp(&b.recall).unwrap());

    // Collisions: intended X routed Y (Y a skill, Y != X), most frequent first.
    let mut collisions: Vec<Collision> = Vec::new();
    for row in &matrix.rows {
        for (j, &count) in row.counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let routed = &labels[j];
            if routed == &row.intended || routed == NONE_LABEL {
                continue;
            }
            collisions.push(Collision {
                intended: row.intended.clone(),
                routed: routed.clone(),
                count,
                intended_total: row.total,
            });
        }
    }
    collisions.sort_by(|a, b| b.count.cmp(&a.count));

    let mut latent: Vec<LatentCollision> = latent
        .into_iter()
        .map(|((skill, also), count)| LatentCollision { skill, also, count })
        .collect();
    latent.sort_by(|a, b| b.count.cmp(&a.count));

    EvalReport {
        matrix,
        metrics,
        collisions,
        latent,
        total_cases: total,
        correct,
        accuracy: ratio(correct, total),
    }
}

fn ratio(num: u32, den: u32) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skillsmith::eval::testset::{CaseSource, TriggerCase};

    fn case(id: u32, intended: Option<&str>) -> TriggerCase {
        TriggerCase {
            id,
            query: format!("q{id}"),
            intended: intended.map(String::from),
            source: CaseSource::Generated,
            locked: false,
        }
    }
    fn routed(id: u32, to: Option<&str>) -> RouteResult {
        RouteResult {
            case_id: id,
            routed: to.map(String::from),
            also_plausible: vec![],
        }
    }

    #[test]
    fn confusion_matrix_and_metrics() {
        let names = vec!["react-testing".to_string(), "angular-testing".to_string()];
        // 10 queries meant for react-testing; 7 wrongly routed to angular.
        let mut cases = Vec::new();
        let mut results = Vec::new();
        for i in 1..=10 {
            cases.push(case(i, Some("react-testing")));
            let to = if i <= 7 { "angular-testing" } else { "react-testing" };
            results.push(routed(i, Some(to)));
        }
        let report = build_report(&names, &cases, &results);

        // Headline collision: react-testing -> angular-testing, 7.
        let top = &report.collisions[0];
        assert_eq!(top.intended, "react-testing");
        assert_eq!(top.routed, "angular-testing");
        assert_eq!(top.count, 7);
        assert_eq!(top.intended_total, 10);

        let react = report.metrics.iter().find(|m| m.name == "react-testing").unwrap();
        assert_eq!(react.tp, 3);
        assert_eq!(react.fn_, 7);
        assert!((react.recall - 0.3).abs() < 1e-9);

        let angular = report.metrics.iter().find(|m| m.name == "angular-testing").unwrap();
        assert_eq!(angular.fp, 7);
        assert!((angular.precision - 0.0).abs() < 1e-9);

        assert_eq!(report.total_cases, 10);
        assert_eq!(report.correct, 3);
    }

    #[test]
    fn near_negatives_and_latent_overlap() {
        let names = vec!["pdf".to_string(), "docx".to_string()];
        let cases = vec![case(1, None), case(2, Some("pdf"))];
        let results = vec![
            // a near-negative wrongly fires pdf
            routed(1, Some("pdf")),
            // correct, but docx also plausible → latent overlap
            RouteResult {
                case_id: 2,
                routed: Some("pdf".into()),
                also_plausible: vec!["docx".into()],
            },
        ];
        let report = build_report(&names, &cases, &results);

        // The negative firing pdf shows as a collision from NONE.
        assert!(report
            .collisions
            .iter()
            .any(|c| c.intended == NONE_LABEL && c.routed == "pdf" && c.count == 1));
        // Latent overlap pdf~docx surfaces even though argmax was correct.
        assert!(report
            .latent
            .iter()
            .any(|l| l.skill == "pdf" && l.also == "docx" && l.count == 1));
    }
}
