//! Fixture-driven validator tests. Each case is a data fixture: a skill folder
//! name, a set of files to write, and the exact set of diagnostic CODES the
//! validator must produce (order-independent). Cases are materialized to a temp
//! dir and validated against the real filesystem walker.

use std::collections::BTreeSet;
use std::path::Path;

use lighter_lib::skillsmith::validate_skill;

/// (relative path within the skill dir, raw bytes)
type File = (&'static str, Vec<u8>);

struct Case {
    /// Skill folder name (the `name` field must match this after NFKC).
    folder: &'static str,
    files: Vec<File>,
    expected: &'static [&'static str],
}

fn f(path: &'static str, content: impl AsRef<[u8]>) -> File {
    (path, content.as_ref().to_vec())
}

fn skill_md(name: &str, description: &str, body: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n")
}

fn run(case: &Case) {
    let root = std::env::temp_dir().join(format!(
        "skillsmith-fx-{}-{}",
        case.folder.replace(['-', '/', '\\'], "_"),
        uuid::Uuid::new_v4()
    ));
    let skill_dir = root.join(case.folder);
    std::fs::create_dir_all(&skill_dir).unwrap();
    for (rel, content) in &case.files {
        let path = skill_dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }

    let report = validate_skill(&skill_dir);
    let got: BTreeSet<String> = report.codes().into_iter().collect();
    let want: BTreeSet<String> = case.expected.iter().map(|s| s.to_string()).collect();

    assert_eq!(
        got, want,
        "\ncase '{}':\n  expected: {:?}\n  got:      {:?}\n  diagnostics: {:#?}",
        case.folder, want, got, report.diagnostics
    );

    let _ = std::fs::remove_dir_all(&root);
}

fn valid(folder: &'static str) -> String {
    skill_md(
        folder,
        "Does a focused thing. Use when the user is doing that focused thing.",
        "# Title\n\nDo the thing.",
    )
}

#[test]
fn validator_matches_fixtures() {
    let long_desc = "d".repeat(1025);
    let compat_long = "c".repeat(501);
    let long_name: &'static str = Box::leak("a".repeat(65).into_boxed_str());
    let many_lines = (0..520).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    let many_tokens = (0..400)
        .map(|i| format!("Sentence number {i} has a fair amount of ordinary english words in it."))
        .collect::<Vec<_>>()
        .join("\n");

    let cases: Vec<Case> = vec![
        // ---- valid ---------------------------------------------------------
        Case {
            folder: "pdf-tools",
            files: vec![f("SKILL.md", valid("pdf-tools"))],
            expected: &[],
        },
        Case {
            folder: "with-refs",
            files: vec![
                f(
                    "SKILL.md",
                    skill_md(
                        "with-refs",
                        "Bundles a guide and a script. Use when bundling refs.",
                        "See references/guide.md and run scripts/run.py to do it.",
                    ),
                ),
                f("references/guide.md", "# Guide"),
                f("scripts/run.py", "print('hi')"),
            ],
            expected: &[],
        },
        // ---- frontmatter / format -----------------------------------------
        Case {
            folder: "bom-skill",
            files: vec![f(
                "SKILL.md",
                {
                    let mut b = vec![0xEF, 0xBB, 0xBF];
                    b.extend_from_slice(valid("bom-skill").as_bytes());
                    b
                },
            )],
            expected: &["FRONT_BOM"],
        },
        Case {
            folder: "no-front",
            files: vec![f("SKILL.md", "# Just a heading\n\nNo frontmatter here.\n")],
            expected: &["FRONT_MISSING"],
        },
        Case {
            folder: "not-byte0",
            files: vec![f(
                "SKILL.md",
                "\n---\nname: not-byte0\ndescription: x. use when x.\n---\n\nbody\n",
            )],
            expected: &["FRONT_NOT_BYTE0"],
        },
        Case {
            folder: "unclosed",
            files: vec![f("SKILL.md", "---\nname: unclosed\ndescription: x. use when x.\n\nbody\n")],
            expected: &["FRONT_UNCLOSED"],
        },
        Case {
            folder: "bad-yaml",
            files: vec![f(
                "SKILL.md",
                "---\nname: \"unterminated\ndescription: x\n---\n\nbody\n",
            )],
            expected: &["YAML_INVALID"],
        },
        Case {
            folder: "dup-key",
            files: vec![f(
                "SKILL.md",
                "---\nname: dup-key\nname: dup-key\ndescription: x. use when x.\n---\n\nbody\n",
            )],
            expected: &["YAML_DUP_KEY"],
        },
        // ---- schema / keys -------------------------------------------------
        Case {
            folder: "unknown-key",
            files: vec![f(
                "SKILL.md",
                "---\nname: unknown-key\ndescription: x. use when x.\nfoo: bar\n---\n\nbody\n",
            )],
            expected: &["KEY_UNKNOWN"],
        },
        Case {
            folder: "no-name",
            files: vec![f("SKILL.md", "---\ndescription: x. use when x.\n---\n\nbody\n")],
            expected: &["KEY_MISSING_NAME"],
        },
        Case {
            folder: "no-desc",
            files: vec![f("SKILL.md", "---\nname: no-desc\n---\n\nbody\n")],
            expected: &["KEY_MISSING_DESCRIPTION"],
        },
        // ---- name ----------------------------------------------------------
        Case {
            folder: "pdf_tools",
            files: vec![f("SKILL.md", valid("pdf_tools"))],
            expected: &["NAME_CHARSET"],
        },
        Case {
            folder: "-pdf",
            files: vec![f("SKILL.md", valid("-pdf"))],
            expected: &["NAME_HYPHEN_EDGE"],
        },
        Case {
            folder: "pdf--tools",
            files: vec![f("SKILL.md", valid("pdf--tools"))],
            expected: &["NAME_HYPHEN_DOUBLE"],
        },
        Case {
            folder: "claude-helper",
            files: vec![f("SKILL.md", valid("claude-helper"))],
            expected: &["NAME_RESERVED"],
        },
        Case {
            folder: long_name,
            files: vec![f("SKILL.md", valid(long_name))],
            expected: &["NAME_TOO_LONG"],
        },
        Case {
            folder: "pdf-tools-x",
            files: vec![f(
                "SKILL.md",
                skill_md("pdf-tools-y", "x. use when x.", "body"),
            )],
            expected: &["NAME_FOLDER_MISMATCH"],
        },
        // ---- description / compatibility ----------------------------------
        Case {
            folder: "desc-long",
            files: vec![f("SKILL.md", skill_md("desc-long", &long_desc, "body"))],
            expected: &["DESC_TOO_LONG"],
        },
        Case {
            folder: "compat-long",
            files: vec![f(
                "SKILL.md",
                format!(
                    "---\nname: compat-long\ndescription: x. use when x.\ncompatibility: {compat_long}\n---\n\nbody\n"
                ),
            )],
            expected: &["COMPAT_TOO_LONG"],
        },
        // ---- body ----------------------------------------------------------
        Case {
            folder: "body-lines",
            files: vec![f("SKILL.md", skill_md("body-lines", "x. use when x.", &many_lines))],
            expected: &["BODY_TOO_MANY_LINES"],
        },
        Case {
            folder: "body-tokens",
            files: vec![f("SKILL.md", skill_md("body-tokens", "x. use when x.", &many_tokens))],
            expected: &["BODY_TOO_MANY_TOKENS"],
        },
        // ---- files / references -------------------------------------------
        Case {
            folder: "dead-ref",
            files: vec![f(
                "SKILL.md",
                skill_md("dead-ref", "x. use when x.", "See references/missing.md for details."),
            )],
            expected: &["REF_DEAD"],
        },
        Case {
            // Unreferenced references/ doc is a hard error (genuinely dead).
            folder: "orphan-ref",
            files: vec![
                f("SKILL.md", skill_md("orphan-ref", "x. use when x.", "No references here.")),
                f("references/orphan.md", "# Orphan"),
            ],
            expected: &["REF_UNREFERENCED"],
        },
        Case {
            // A truly dead script (no body ref, not imported) is a hard error;
            // __init__.py is exempt.
            folder: "orphan-script",
            files: vec![
                f("SKILL.md", skill_md("orphan-script", "x. use when x.", "body")),
                f("scripts/helper.py", "x = 1"),
                f("scripts/__init__.py", ""),
            ],
            expected: &["REF_UNREFERENCED"],
        },
        Case {
            // A helper reachable via import from a referenced entry point is
            // NOT flagged — the import graph is traced.
            folder: "reachable-script",
            files: vec![
                f(
                    "SKILL.md",
                    skill_md("reachable-script", "x. use when x.", "Run scripts/main.py to do it."),
                ),
                f("scripts/main.py", "import helper\nhelper.go()\n"),
                f("scripts/helper.py", "def go(): pass\n"),
            ],
            expected: &[],
        },
        Case {
            // Python module notation counts as a reference — no false positive.
            folder: "module-ref",
            files: vec![
                f(
                    "SKILL.md",
                    skill_md("module-ref", "x. use when x.", "Run `python -m scripts.run_loop`."),
                ),
                f("scripts/run_loop.py", "print(1)"),
            ],
            expected: &[],
        },
        Case {
            folder: "too-deep",
            files: vec![
                f("SKILL.md", skill_md("too-deep", "x. use when x.", "body")),
                f("references/db/v1/schema.md", "# Schema"),
            ],
            expected: &["FILE_TOO_DEEP"],
        },
        // ---- filename ------------------------------------------------------
        Case {
            folder: "lower-name",
            files: vec![f("skill.md", valid("lower-name"))],
            expected: &["FILENAME_LOWERCASE"],
        },
        Case {
            folder: "no-skillmd",
            files: vec![f("README.md", "not a skill")],
            expected: &["FILENAME_MISSING"],
        },
    ];

    for case in &cases {
        run(case);
    }
}

#[test]
fn ok_flag_reflects_error_severity() {
    let root = std::env::temp_dir().join(format!("skillsmith-ok-{}", uuid::Uuid::new_v4()));
    let dir = root.join("ok-skill");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), valid("ok-skill")).unwrap();
    assert!(validate_skill(&dir).ok);

    // A warning-only skill (long body) is still ok=true.
    let dir2 = root.join("warn-skill");
    std::fs::create_dir_all(&dir2).unwrap();
    let body = (0..520).map(|i| format!("l{i}")).collect::<Vec<_>>().join("\n");
    std::fs::write(dir2.join("SKILL.md"), skill_md("warn-skill", "x. use when x.", &body)).unwrap();
    let report = validate_skill(&dir2);
    assert!(report.ok, "warnings must not flip ok to false: {:?}", report.diagnostics);

    // A script referenced from the body is fine; a dead one is an error.
    let dir3 = root.join("script-skill");
    std::fs::create_dir_all(dir3.join("scripts")).unwrap();
    std::fs::write(
        dir3.join("SKILL.md"),
        skill_md("script-skill", "x. use when x.", "Run scripts/helper.py."),
    )
    .unwrap();
    std::fs::write(dir3.join("scripts").join("helper.py"), "x=1").unwrap();
    assert!(validate_skill(&dir3).ok);

    let dir4 = root.join("dead-script-skill");
    std::fs::create_dir_all(dir4.join("scripts")).unwrap();
    std::fs::write(dir4.join("SKILL.md"), skill_md("dead-script-skill", "x. use when x.", "body")).unwrap();
    std::fs::write(dir4.join("scripts").join("dead.py"), "x=1").unwrap();
    assert!(!validate_skill(&dir4).ok, "a dead script must fail validation");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn strict_escalates_body_warnings_to_errors() {
    use lighter_lib::skillsmith::{validate_skill_with, ValidateOptions};
    let root = std::env::temp_dir().join(format!("skillsmith-strict-{}", uuid::Uuid::new_v4()));
    let dir = root.join("big-body");
    std::fs::create_dir_all(&dir).unwrap();
    let body = (0..520).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    std::fs::write(dir.join("SKILL.md"), skill_md("big-body", "x. use when x.", &body)).unwrap();

    assert!(validate_skill(&dir).ok, "lenient: long body is a warning");
    assert!(
        !validate_skill_with(&dir, ValidateOptions { strict: true }).ok,
        "strict: long body is an error"
    );
    let _ = std::fs::remove_dir_all(&root);
}

fn _unused(_p: &Path) {}
