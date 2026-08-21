use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::runner::{Diagnostic, RunReport, Status};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Baseline {
    version: u8,
    findings: Vec<BaselineFinding>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BaselineFinding {
    fingerprint: String,
    tool: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    rule: Option<String>,
    message: String,
    occurrences: usize,
}

#[derive(Clone, Debug)]
pub struct BaselineSummary {
    pub findings: usize,
    pub occurrences: usize,
}

pub fn create(report: &RunReport, path: &Path, force: bool) -> Result<BaselineSummary> {
    if path.exists() && !force {
        anyhow::bail!(
            "{} already exists; pass --force to replace it",
            path.display()
        );
    }
    let unsafe_failures: Vec<_> = report
        .results
        .iter()
        .filter(|result| {
            (matches!(result.status, Status::Missing) && result.guidance.is_some())
                || (matches!(result.status, Status::Failed)
                    && (!result.baseline_safe || !result.diagnostics.iter().any(eligible)))
        })
        .map(|result| result.name.as_str())
        .collect();
    if !unsafe_failures.is_empty() {
        anyhow::bail!(
            "cannot create a safe baseline because these tools did not produce file findings: {}; run `quality doctor` and fix their execution first",
            unsafe_failures.join(", ")
        );
    }

    let mut findings: BTreeMap<String, BaselineFinding> = BTreeMap::new();
    let mut occurrences = 0;
    for diagnostic in report
        .results
        .iter()
        .flat_map(|result| &result.diagnostics)
        .filter(|diagnostic| eligible(diagnostic))
    {
        occurrences += 1;
        let fingerprint = fingerprint(diagnostic);
        findings
            .entry(fingerprint.clone())
            .and_modify(|finding| finding.occurrences += 1)
            .or_insert_with(|| BaselineFinding {
                fingerprint,
                tool: diagnostic.tool.clone(),
                path: diagnostic.path.clone().unwrap_or_default(),
                rule: diagnostic.rule.clone(),
                message: diagnostic.message.clone(),
                occurrences: 1,
            });
    }
    let baseline = Baseline {
        version: 1,
        findings: findings.into_values().collect(),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create baseline directory {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_vec_pretty(&baseline)?)
        .with_context(|| format!("could not write baseline to {}", path.display()))?;
    Ok(BaselineSummary {
        findings: baseline.findings.len(),
        occurrences,
    })
}

pub fn apply(report: &mut RunReport, path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let bytes = fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    let baseline: Baseline = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid baseline in {}", path.display()))?;
    if baseline.version != 1 {
        anyhow::bail!(
            "unsupported baseline version {} in {}; expected 1",
            baseline.version,
            path.display()
        );
    }
    let mut remaining: BTreeMap<_, _> = baseline
        .findings
        .into_iter()
        .map(|finding| (finding.fingerprint, finding.occurrences))
        .collect();

    for result in &mut report.results {
        let before = result.diagnostics.len();
        result.diagnostics.retain(|diagnostic| {
            let fingerprint = fingerprint(diagnostic);
            let Some(count) = remaining.get_mut(&fingerprint) else {
                return true;
            };
            if *count == 0 {
                return true;
            }
            *count -= 1;
            false
        });
        let suppressed = before - result.diagnostics.len();
        report.suppressed += suppressed;
        if suppressed > 0
            && result.diagnostics.is_empty()
            && matches!(result.status, Status::Failed)
        {
            result.status = Status::Passed;
        }
    }
    Ok(())
}

fn eligible(diagnostic: &Diagnostic) -> bool {
    diagnostic.path.is_some() && diagnostic.rule.as_deref() != Some("tool-not-installed")
}

fn fingerprint(diagnostic: &Diagnostic) -> String {
    // Stable FNV-1a. Lines and columns are intentionally excluded so harmless code
    // movement does not turn an existing finding into a new regression.
    let mut hash = 0xcbf29ce484222325_u64;
    for value in [
        Some(diagnostic.tool.as_str()),
        diagnostic.path.as_deref(),
        diagnostic.rule.as_deref(),
        Some(diagnostic.severity.as_str()),
        Some(diagnostic.message.as_str()),
    ] {
        for byte in value.unwrap_or("").bytes().chain(std::iter::once(0)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{RunReport, ToolResult};

    fn diagnostic(line: u64) -> Diagnostic {
        Diagnostic {
            tool: "swiftlint".to_owned(),
            path: Some("App.swift".to_owned()),
            line: Some(line),
            column: Some(1),
            severity: "warning".to_owned(),
            message: "Example".to_owned(),
            rule: Some("example".to_owned()),
        }
    }

    #[test]
    fn fingerprint_survives_line_movement() {
        assert_eq!(fingerprint(&diagnostic(2)), fingerprint(&diagnostic(200)));
    }

    #[test]
    fn occurrence_counts_do_not_hide_new_duplicates() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("baseline.json");
        let initial = RunReport {
            results: vec![result(vec![diagnostic(2)])],
            scope: None,
            suppressed: 0,
        };
        create(&initial, &path, false).unwrap();

        let mut current = RunReport {
            results: vec![result(vec![diagnostic(4), diagnostic(8)])],
            scope: None,
            suppressed: 0,
        };
        apply(&mut current, &path).unwrap();
        assert_eq!(current.suppressed, 1);
        assert_eq!(current.results[0].diagnostics.len(), 1);
        assert!(matches!(current.results[0].status, Status::Failed));
    }

    fn result(diagnostics: Vec<Diagnostic>) -> ToolResult {
        ToolResult {
            tool: "swiftlint".to_owned(),
            name: "SwiftLint".to_owned(),
            status: Status::Failed,
            duration_ms: 1,
            command: "swiftlint".to_owned(),
            diagnostics,
            output: String::new(),
            guidance: None,
            baseline_safe: true,
        }
    }
}
