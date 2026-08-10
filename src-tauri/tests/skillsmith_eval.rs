//! Eval core tests that don't need a model: the catalog never leaks skill
//! body into the routing prompt, and test-set regeneration preserves
//! hand-authored cases.

use std::path::Path;

use lighter_lib::skillsmith::eval::catalog::{build_catalog, routing_system_prompt};
use lighter_lib::skillsmith::eval::testset::{CaseSource, TriggerCase, TriggerSet};

fn write_skill(root: &Path, name: &str, description: &str, body: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n"),
    )
    .unwrap();
}

#[test]
fn routing_prompt_contains_metadata_never_body() {
    let root = std::env::temp_dir().join(format!("sks-cat-{}", uuid::Uuid::new_v4()));
    // A distinctive body marker that must NEVER appear in the routing prompt.
    let marker = "ZZZ_SECRET_BODY_MARKER_9137_do_not_leak";
    write_skill(
        &root,
        "pdf-tools",
        "Extract and fill PDFs. Use when the user mentions PDFs or forms.",
        &format!("# PDF\n\n{marker}\n\nStep-by-step body the router must not see."),
    );
    write_skill(
        &root,
        "xlsx-tools",
        "Edit spreadsheets. Use for xlsx, formulas, pivot tables.",
        "# XLSX body also hidden",
    );

    let catalog = build_catalog(&root);
    assert_eq!(catalog.len(), 2);
    assert!(catalog.iter().any(|s| s.name == "pdf-tools"));

    let prompt = routing_system_prompt(&catalog);
    // Names + descriptions present…
    assert!(prompt.contains("pdf-tools"));
    assert!(prompt.contains("Use when the user mentions PDFs"));
    assert!(prompt.contains("xlsx-tools"));
    // …body absolutely absent.
    assert!(
        !prompt.contains(marker),
        "routing prompt leaked skill body: {prompt}"
    );
    assert!(!prompt.contains("Step-by-step body"));

    let _ = std::fs::remove_dir_all(&root);
}

fn case(query: &str, source: CaseSource, locked: bool) -> TriggerCase {
    TriggerCase {
        id: 0,
        query: query.to_string(),
        intended: None,
        source,
        locked,
    }
}

#[test]
fn regeneration_preserves_manual_and_locked_cases() {
    let existing = TriggerSet {
        version: 1,
        cases: vec![
            case("hand written by user", CaseSource::Manual, false),
            case("locked generated case", CaseSource::Generated, true),
            case("old generated case", CaseSource::Generated, false),
        ],
    };

    let generated = vec![
        // Duplicate of a preserved query — must not be added twice.
        case("HAND WRITTEN BY USER", CaseSource::Generated, false),
        case("fresh generated one", CaseSource::Generated, false),
    ];

    let merged = existing.merge_generated(generated);
    let queries: Vec<&str> = merged.cases.iter().map(|c| c.query.as_str()).collect();

    // Manual + locked preserved; old non-locked generated dropped; fresh added.
    assert!(queries.contains(&"hand written by user"));
    assert!(queries.contains(&"locked generated case"));
    assert!(!queries.contains(&"old generated case"));
    assert!(queries.contains(&"fresh generated one"));
    // No duplicate of the preserved manual query.
    assert_eq!(
        merged.cases.iter().filter(|c| c.query.to_lowercase() == "hand written by user").count(),
        1
    );
    // Ids are sequential.
    assert_eq!(merged.cases.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1, 2, 3]);
}
