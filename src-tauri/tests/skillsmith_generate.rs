//! Scaffolding is the only filesystem-touching part of generation: it writes
//! one skill, refuses personas, and validates immediately.

use lighter_lib::skillsmith::generate::{scaffold, SkillSpec};

fn tmp() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sks-gen-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn scaffolds_a_valid_skill() {
    let parent = tmp();
    let spec = SkillSpec {
        name: "pdf-form-filler".into(),
        description: "Fill our internal PDF forms from a data row. Use when the user needs to \
populate a company PDF form or mentions form filling.".into(),
        body: "# PDF Form Filler\n\nMap each column to a field, then flatten.".into(),
    };
    let res = scaffold(&parent, &spec).unwrap();
    assert!(res.report.ok, "diagnostics: {:?}", res.report.diagnostics);
    assert_eq!(res.report.name.as_deref(), Some("pdf-form-filler"));
    assert!(std::path::Path::new(&res.skill_dir).join("SKILL.md").is_file());

    // Creating over an existing folder is refused.
    assert!(scaffold(&parent, &spec).is_err());

    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn refuses_persona_content() {
    let parent = tmp();
    let spec = SkillSpec {
        name: "sec-review".into(),
        description: "You are a senior security engineer. Review code for vulnerabilities.".into(),
        body: "Look for issues.".into(),
    };
    let err = scaffold(&parent, &spec).unwrap_err().to_string();
    assert!(err.to_lowercase().contains("persona"), "unexpected error: {err}");
    // Nothing was written.
    assert!(!parent.join("sec-review").exists());

    let _ = std::fs::remove_dir_all(&parent);
}
