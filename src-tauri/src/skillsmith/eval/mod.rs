//! Trigger eval: does each skill's `description` actually route the right
//! queries to it — and NOT steal queries meant for the user's other skills?
//! The pure pieces (catalog, test set, confusion matrix) are here and unit
//! tested; the model calls and orchestration live in `model` / `run`.

pub mod catalog;
pub mod model;
pub mod report;
pub mod run;
pub mod testset;

pub use catalog::{build_catalog, routing_system_prompt, skill_names, SkillMeta};
pub use report::{build_report, EvalReport, RouteResult};
pub use testset::{CaseSource, TriggerCase, TriggerSet};
