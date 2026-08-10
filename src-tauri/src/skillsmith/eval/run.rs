//! Eval orchestration: run the router over the test set (with a response
//! cache so re-runs are cheap and deterministic), generate a starter test set,
//! and propose — never auto-apply — a revised description, with the metric
//! delta measured on the same test set.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Error, Result};

use super::catalog::{build_catalog, routing_system_prompt, skill_names, SkillMeta};
use super::model::Model;
use super::report::{build_report, EvalReport, RouteResult, SkillMetrics};
use super::testset::{CaseSource, TriggerCase, TriggerSet};

pub fn testset_path(skills_dir: &Path) -> PathBuf {
    skills_dir.join(".skillsmith").join("trigger.yaml")
}

fn cache_path(skills_dir: &Path) -> PathBuf {
    skills_dir.join(".skillsmith").join("route-cache.json")
}

fn cache_key(system: &str, query: &str, kind: &str) -> String {
    let mut h = DefaultHasher::new();
    system.hash(&mut h);
    format!("{:x}|{kind}|{query}", h.finish())
}

/// Run the router over every case, using and updating a response cache.
pub async fn run_eval(
    skills_dir: &Path,
    testset: &TriggerSet,
    model: &Model,
) -> Result<EvalReport> {
    let catalog = build_catalog(skills_dir);
    let names = skill_names(&catalog);
    let system = routing_system_prompt(&catalog);

    let cache_file = cache_path(skills_dir);
    let mut cache: HashMap<String, RouteCacheEntry> = std::fs::read_to_string(&cache_file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let mut results = Vec::with_capacity(testset.cases.len());
    for case in &testset.cases {
        let key = cache_key(&system, &case.query, model.kind());
        let entry = if let Some(hit) = cache.get(&key) {
            hit.clone()
        } else {
            let decision = model.route(&system, &case.query).await?;
            let entry = RouteCacheEntry {
                skill: decision.skill,
                also_plausible: decision.also_plausible,
            };
            cache.insert(key, entry.clone());
            entry
        };
        results.push(RouteResult {
            case_id: case.id,
            routed: entry.skill,
            also_plausible: entry.also_plausible,
        });
    }

    if let Some(parent) = cache_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        &cache_file,
        serde_json::to_string_pretty(&cache).unwrap_or_default(),
    );

    Ok(build_report(&names, &testset.cases, &results))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RouteCacheEntry {
    skill: Option<String>,
    also_plausible: Vec<String>,
}

/// Ask the model for a starter test set (should-trigger + near-negatives per
/// skill), then MERGE it into whatever is on disk — hand-authored / locked
/// cases are never touched.
pub async fn generate_testset(skills_dir: &Path, model: &Model) -> Result<TriggerSet> {
    let catalog = build_catalog(skills_dir);
    if catalog.is_empty() {
        return Err(Error::InvalidInput("no skills found to build a test set".into()));
    }
    let catalog_text = catalog
        .iter()
        .map(|s| format!("- {}: {}", s.name, s.description))
        .collect::<Vec<_>>()
        .join("\n");

    let mut generated: Vec<TriggerCase> = Vec::new();
    for skill in &catalog {
        let system = format!(
            "You write realistic trigger-eval queries for a Claude Code skill. Here is the full \
skill catalog:\n{catalog_text}\n\nQueries must be concrete and specific — the kind of thing a real \
user types (file paths, column names, company names, casual phrasing, sometimes typos). Avoid \
generic one-liners.",
        );
        let user = format!(
            "For the skill '{}' ({}), produce JSON with two arrays of 5 queries each:\n\
- \"should_trigger\": queries that clearly need THIS skill (varied phrasing, some not naming it).\n\
- \"near_negatives\": tricky queries that SHARE keywords/topic but should NOT use this skill \
(adjacent domains, or where another tool fits) — genuinely hard, not obviously irrelevant.\n\
Return only {{\"should_trigger\": [...], \"near_negatives\": [...]}}.",
            skill.name, skill.description
        );
        let value = model.complete_json(&system, &user).await?;
        for q in value["should_trigger"].as_array().into_iter().flatten() {
            if let Some(text) = q.as_str() {
                generated.push(mk_case(text, Some(skill.name.clone())));
            }
        }
        for q in value["near_negatives"].as_array().into_iter().flatten() {
            if let Some(text) = q.as_str() {
                generated.push(mk_case(text, None));
            }
        }
    }

    let path = testset_path(skills_dir);
    let existing = TriggerSet::load(&path);
    let merged = existing.merge_generated(generated);
    merged
        .save(&path)
        .map_err(|e| Error::Control(format!("could not save test set: {e}")))?;
    Ok(merged)
}

fn mk_case(query: &str, intended: Option<String>) -> TriggerCase {
    TriggerCase {
        id: 0,
        query: query.to_string(),
        intended,
        source: CaseSource::Generated,
        locked: false,
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DescriptionFix {
    pub skill: String,
    pub old: String,
    pub new: String,
    pub before: SkillMetrics,
    pub after: SkillMetrics,
    pub before_collisions: u32,
    pub after_collisions: u32,
}

/// Propose a revised `description` for a failing skill and measure the delta on
/// the same test set. Does NOT write anything.
pub async fn propose_fix(
    skills_dir: &Path,
    skill: &str,
    testset: &TriggerSet,
    model: &Model,
) -> Result<DescriptionFix> {
    let catalog = build_catalog(skills_dir);
    let current = catalog
        .iter()
        .find(|s| s.name == skill)
        .ok_or_else(|| Error::InvalidInput(format!("no such skill: {skill}")))?
        .clone();

    let before = run_eval(skills_dir, testset, model).await?;
    let before_metrics = metrics_for(&before, skill);
    let before_collisions = collisions_touching(&before, skill);

    // Failing examples grounded from the (cached) eval, disambiguating vs siblings.
    let (missed, stolen, false_pos) = failing_examples(skills_dir, testset, model, skill).await?;
    let siblings = catalog
        .iter()
        .filter(|s| s.name != skill)
        .map(|s| format!("- {}: {}", s.name, s.description))
        .collect::<Vec<_>>()
        .join("\n");

    let system = String::from(
        "You improve a Claude Code skill's `description` — the ONLY text that decides whether the \
skill triggers. Make it fire on the queries it missed and stop stealing queries meant for sibling \
skills. Be specific about WHEN to use it and, implicitly, how it differs from siblings. Keep it \
under 1024 characters. Do not invent a persona. Return only {\"description\": \"...\"}.",
    );
    let user = format!(
        "Skill: {skill}\nCurrent description: {}\n\nSibling skills:\n{siblings}\n\n\
Queries it SHOULD have triggered on but didn't:\n{}\n\n\
Queries meant for other skills that it wrongly stole:\n{}\n\n\
Near-negative queries it wrongly fired on:\n{}\n",
        current.description,
        bullet(&missed),
        bullet(&stolen),
        bullet(&false_pos),
    );
    let value = model.complete_json(&system, &user).await?;
    let new_desc = value["description"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Control("model did not return a description".into()))?;

    let after = eval_with_override(skills_dir, testset, model, skill, &new_desc).await?;

    Ok(DescriptionFix {
        skill: skill.to_string(),
        old: current.description,
        new: new_desc,
        before: before_metrics,
        after: metrics_for(&after, skill),
        before_collisions,
        after_collisions: collisions_touching(&after, skill),
    })
}

/// Run the eval with one skill's description swapped in memory (no disk write).
async fn eval_with_override(
    skills_dir: &Path,
    testset: &TriggerSet,
    model: &Model,
    skill: &str,
    new_desc: &str,
) -> Result<EvalReport> {
    let mut catalog = build_catalog(skills_dir);
    for s in &mut catalog {
        if s.name == skill {
            s.description = new_desc.to_string();
        }
    }
    let names = skill_names(&catalog);
    let system = routing_system_prompt(&catalog);
    let mut results = Vec::new();
    for case in &testset.cases {
        let decision = model.route(&system, &case.query).await?;
        results.push(RouteResult {
            case_id: case.id,
            routed: decision.skill,
            also_plausible: decision.also_plausible,
        });
    }
    Ok(build_report(&names, &testset.cases, &results))
}

/// Apply an accepted description to the skill's SKILL.md frontmatter.
/// Returns the skill directory (for re-validation).
pub fn apply_description(skills_dir: &Path, skill: &str, new_desc: &str) -> Result<PathBuf> {
    let dir = find_skill_dir(skills_dir, skill)
        .ok_or_else(|| Error::InvalidInput(format!("no such skill on disk: {skill}")))?;
    let file = ["SKILL.md", "skill.md"]
        .iter()
        .map(|f| dir.join(f))
        .find(|p| p.is_file())
        .ok_or_else(|| Error::InvalidInput("skill has no SKILL.md".into()))?;
    let text = std::fs::read_to_string(&file)?;
    let rewritten = rewrite_description(&text, new_desc)?;
    std::fs::write(&file, rewritten)?;
    Ok(dir)
}

/// Replace the single-line `description:` value inside the frontmatter,
/// YAML-escaping the new value. (Block-scalar descriptions are rare in skills.)
fn rewrite_description(text: &str, new_desc: &str) -> Result<String> {
    let normalized = text.replace('\r', "");
    let lines: Vec<&str> = normalized.split('\n').collect();
    if lines.first().copied() != Some("---") {
        return Err(Error::Control("no frontmatter to update".into()));
    }
    let close = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| **l == "---")
        .map(|(i, _)| i)
        .ok_or_else(|| Error::Control("unterminated frontmatter".into()))?;

    let scalar = serde_yaml::to_string(&serde_json::Value::String(new_desc.to_string()))
        .unwrap_or_else(|_| format!("{new_desc}\n"));
    let scalar = scalar.trim_end();
    let mut out: Vec<String> = Vec::new();
    let mut replaced = false;
    for (i, line) in lines.iter().enumerate() {
        if i > 0 && i < close && line.starts_with("description:") && !replaced {
            out.push(format!("description: {scalar}"));
            replaced = true;
        } else {
            out.push((*line).to_string());
        }
    }
    if !replaced {
        return Err(Error::Control("no description: line found".into()));
    }
    Ok(out.join("\n"))
}

fn find_skill_dir(skills_dir: &Path, skill: &str) -> Option<PathBuf> {
    if crate::skillsmith::validate::parse_meta(skills_dir).map(|(n, _)| n).as_deref() == Some(skill)
    {
        return Some(skills_dir.to_path_buf());
    }
    std::fs::read_dir(skills_dir).ok()?.flatten().find_map(|e| {
        let p = e.path();
        if p.is_dir()
            && crate::skillsmith::validate::parse_meta(&p).map(|(n, _)| n).as_deref() == Some(skill)
        {
            Some(p)
        } else {
            None
        }
    })
}

async fn failing_examples(
    skills_dir: &Path,
    testset: &TriggerSet,
    model: &Model,
    skill: &str,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let catalog = build_catalog(skills_dir);
    let system = routing_system_prompt(&catalog);
    let mut missed = Vec::new();
    let mut stolen = Vec::new();
    let mut false_pos = Vec::new();
    for case in &testset.cases {
        let routed = model.route(&system, &case.query).await?.skill;
        match (&case.intended, &routed) {
            (Some(intended), r) if intended == skill && r.as_deref() != Some(skill) => {
                missed.push(case.query.clone())
            }
            (Some(intended), Some(r)) if intended != skill && r == skill => {
                stolen.push(case.query.clone())
            }
            (None, Some(r)) if r == skill => false_pos.push(case.query.clone()),
            _ => {}
        }
    }
    Ok((missed, stolen, false_pos))
}

fn metrics_for(report: &EvalReport, skill: &str) -> SkillMetrics {
    report
        .metrics
        .iter()
        .find(|m| m.name == skill)
        .cloned()
        .unwrap_or(SkillMetrics {
            name: skill.to_string(),
            intended_total: 0,
            tp: 0,
            fp: 0,
            fn_: 0,
            precision: 0.0,
            recall: 0.0,
        })
}

fn collisions_touching(report: &EvalReport, skill: &str) -> u32 {
    report
        .collisions
        .iter()
        .filter(|c| c.intended == skill || c.routed == skill)
        .map(|c| c.count)
        .sum()
}

fn bullet(items: &[String]) -> String {
    if items.is_empty() {
        "(none)".into()
    } else {
        items.iter().map(|q| format!("- {q}")).collect::<Vec<_>>().join("\n")
    }
}

/// Re-exported so the property test can assert body never leaks into the prompt.
pub fn routing_prompt_for(catalog: &[SkillMeta]) -> String {
    routing_system_prompt(catalog)
}
