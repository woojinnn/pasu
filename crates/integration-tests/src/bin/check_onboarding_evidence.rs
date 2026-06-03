//! Check protocol onboarding evidence markdown.
//!
//! This is intentionally small and markdown-table oriented. The onboarding
//! framework treats `evidence.md` as a phase gate, so this binary fails when a
//! mandatory phase row is still pending, marked with an unsupported status, or
//! claims `done` / `blocked` / `n/a` without an artifact. Valid statuses are
//! `done`, `blocked`, and `n/a` (each requires a justification cell).

use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};

const EVIDENCE_TEMPLATE: &str = include_str!("../../ONBOARDING_EVIDENCE_TEMPLATE.md");

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Section {
    Metadata,
    P0,
    P1,
    P2Synthetic,
    P2Real,
    P3,
    P4,
    Blockers,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    P0,
    P1,
    P2,
    P3,
    P4,
    All,
}

impl Phase {
    fn parse(raw: &str) -> Result<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "p0" => Ok(Self::P0),
            "p1" => Ok(Self::P1),
            "p2" => Ok(Self::P2),
            "p3" => Ok(Self::P3),
            "p4" => Ok(Self::P4),
            "all" => Ok(Self::All),
            other => Err(anyhow!(
                "invalid --phase `{other}`; expected p0, p1, p2, p3, p4, or all"
            )),
        }
    }

    fn includes(self, section: Section) -> bool {
        match self {
            Self::P0 => section == Section::P0,
            Self::P1 => section == Section::P1,
            Self::P2 => matches!(section, Section::P2Synthetic | Section::P2Real),
            Self::P3 => section == Section::P3,
            Self::P4 => section == Section::P4,
            Self::All => matches!(
                section,
                Section::P0
                    | Section::P1
                    | Section::P2Synthetic
                    | Section::P2Real
                    | Section::P3
                    | Section::P4
            ),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::P0 => "p0",
            Self::P1 => "p1",
            Self::P2 => "p2",
            Self::P3 => "p3",
            Self::P4 => "p4",
            Self::All => "all",
        }
    }
}

#[derive(Debug)]
struct Config {
    path: PathBuf,
    phase: Phase,
}

#[derive(Debug)]
struct Finding {
    line: usize,
    message: String,
}

#[derive(Debug)]
struct Stats {
    checked_rows: usize,
    blocked_rows: usize,
    blocker_rows: usize,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let config = parse_args(env::args().skip(1))?;
    let markdown = fs::read_to_string(&config.path)
        .with_context(|| format!("read {}", config.path.display()))?;
    let (findings, stats) = check_markdown(&markdown, config.phase);
    if !findings.is_empty() {
        eprintln!(
            "onboarding evidence check FAILED: path={} phase={} checked_rows={}",
            config.path.display(),
            config.phase.label(),
            stats.checked_rows
        );
        for finding in findings {
            eprintln!("  line {}: {}", finding.line, finding.message);
        }
        bail!("onboarding evidence is incomplete");
    }

    println!(
        "onboarding evidence OK: path={} phase={} checked_rows={} blocked_rows={} blocker_rows={}",
        config.path.display(),
        config.phase.label(),
        stats.checked_rows,
        stats.blocked_rows,
        stats.blocker_rows
    );
    Ok(())
}

fn parse_args<I>(args: I) -> Result<Config>
where
    I: IntoIterator<Item = String>,
{
    let mut protocol: Option<String> = None;
    let mut path: Option<PathBuf> = None;
    let mut phase = Phase::All;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                usage();
                std::process::exit(0);
            }
            "--path" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow!("--path requires a value"))?;
                path = Some(PathBuf::from(value));
            }
            "--phase" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow!("--phase requires a value"))?;
                phase = Phase::parse(&value)?;
            }
            value if value.starts_with('-') => {
                bail!("unknown flag `{value}`");
            }
            value => {
                if protocol.replace(value.to_owned()).is_some() {
                    bail!("expected one protocol argument");
                }
            }
        }
    }

    let path = match path {
        Some(path) => path,
        None => {
            let protocol = protocol
                .ok_or_else(|| anyhow!("missing protocol argument or --path <evidence.md>"))?;
            PathBuf::from("crates")
                .join("integration-tests")
                .join("onboarding")
                .join(protocol)
                .join("evidence.md")
        }
    };

    Ok(Config { path, phase })
}

fn usage() {
    eprintln!(
        "check-onboarding-evidence — validate onboarding evidence.md\n\n\
         USAGE:\n  \
         check-onboarding-evidence <protocol> [--phase all|p0|p1|p2|p3|p4]\n  \
         check-onboarding-evidence --path <evidence.md> [--phase all|p0|p1|p2|p3|p4]\n\n\
         Status values allowed for mandatory phase rows: done, blocked.\n\
         `done` and `blocked` rows must include an artifact/summary cell.\n\
         Any `blocked` row also requires at least one concrete Blockers table row."
    );
}

fn check_markdown(markdown: &str, phase: Phase) -> (Vec<Finding>, Stats) {
    let mut section = Section::Other;
    // Only rows inside a phase STATUS table (its header has a "status" column) are
    // mandatory evidence rows. Explanatory tables/matrices placed inside a phase
    // section (e.g. an L3 dropped-field disposition, a coverage matrix) must NOT be
    // parsed as status rows. This keeps the checker protocol-agnostic to arbitrary
    // evidence prose/tables: `### subsections` do not reset the `## ` section, so
    // without this guard any 3-column table under a phase heading was mis-validated.
    let mut in_status_table = false;
    let mut findings = Vec::new();
    let mut checked_rows = 0usize;
    let mut blocked_rows = 0usize;
    let mut blocker_rows = 0usize;
    let mut seen_requirements: Vec<(Section, String)> = Vec::new();

    for (idx, line) in markdown.lines().enumerate() {
        let line_no = idx + 1;
        if let Some(next) = section_from_heading(line) {
            section = next;
            in_status_table = false;
            continue;
        }

        let Some(cells) = parse_table_row(line) else {
            in_status_table = false;
            continue;
        };
        if is_header_or_separator(&cells) {
            // Arm validation only inside a status table — detected by a "status"
            // column in the header row. Separator rows (all dashes) leave the flag
            // unchanged so the data rows after the header are still validated.
            if cells
                .get(1)
                .is_some_and(|cell| cell.trim().eq_ignore_ascii_case("status"))
            {
                in_status_table = true;
            }
            continue;
        }

        if section == Section::Metadata {
            if cells.len() >= 2 && cells[1].trim().is_empty() {
                findings.push(Finding {
                    line: line_no,
                    message: format!("run metadata `{}` is empty", cells[0]),
                });
            }
            continue;
        }

        if section == Section::Blockers {
            if cells.len() >= 3
                && !cells[0].trim().is_empty()
                && !cells[1].trim().is_empty()
                && !cells[2].trim().is_empty()
            {
                blocker_rows += 1;
            }
            continue;
        }

        if !phase.includes(section) {
            continue;
        }

        // Skip non-status tables (analysis/matrix) inside a phase section — only
        // rows under a "status"-column header are mandatory phase evidence rows.
        if !in_status_table {
            continue;
        }

        checked_rows += 1;
        if cells.len() < 3 {
            findings.push(Finding {
                line: line_no,
                message:
                    "phase evidence row must have required evidence, status, and artifact cells"
                        .to_owned(),
            });
            continue;
        }

        let requirement = cells[0].trim();
        seen_requirements.push((section, normalize_requirement(requirement)));
        let status = normalize_status(&cells[1]);
        let artifact = cells[2].trim();
        match status.as_str() {
            "done" => {
                if artifact.is_empty() {
                    findings.push(Finding {
                        line: line_no,
                        message: format!("`{requirement}` is done but artifact/summary is empty"),
                    });
                }
            }
            "blocked" => {
                blocked_rows += 1;
                if artifact.is_empty() {
                    findings.push(Finding {
                        line: line_no,
                        message: format!(
                            "`{requirement}` is blocked but artifact/summary is empty"
                        ),
                    });
                }
            }
            // `n/a` is a first-class disposition: many rows end "...or explicitly
            // not applicable", and the template itself authors the no-Codex-lane
            // rows as `n/a`. Accept it like `done` (a settled row that needs a
            // justification cell) — but, unlike `blocked`, it is NOT a blocker, so
            // it does not require a concrete Blockers-table entry.
            "n/a" | "na" | "notapplicable" => {
                if artifact.is_empty() {
                    findings.push(Finding {
                        line: line_no,
                        message: format!(
                            "`{requirement}` is n/a but artifact/summary is empty (justify why it does not apply)"
                        ),
                    });
                }
            }
            "pending" | "todo" | "skipped" | "" => findings.push(Finding {
                line: line_no,
                message: format!("`{requirement}` status `{}` is incomplete", cells[1].trim()),
            }),
            other => findings.push(Finding {
                line: line_no,
                message: format!(
                    "`{requirement}` has unsupported status `{other}`; use done, blocked, or n/a"
                ),
            }),
        }
    }

    if checked_rows == 0 {
        findings.push(Finding {
            line: 1,
            message: format!("no mandatory rows found for phase `{}`", phase.label()),
        });
    }
    for (required_section, required) in template_requirements(phase) {
        let found = seen_requirements
            .iter()
            .any(|(section, seen)| *section == required_section && *seen == required);
        if !found {
            findings.push(Finding {
                line: 1,
                message: format!("missing mandatory row `{required}`"),
            });
        }
    }
    if blocked_rows > 0 && blocker_rows == 0 {
        findings.push(Finding {
            line: 1,
            message: "blocked phase rows require at least one concrete Blockers table row"
                .to_owned(),
        });
    }

    (
        findings,
        Stats {
            checked_rows,
            blocked_rows,
            blocker_rows,
        },
    )
}

fn template_requirements(phase: Phase) -> Vec<(Section, String)> {
    let mut section = Section::Other;
    let mut rows = Vec::new();
    for line in EVIDENCE_TEMPLATE.lines() {
        if let Some(next) = section_from_heading(line) {
            section = next;
            continue;
        }

        if !phase.includes(section) {
            continue;
        }

        let Some(cells) = parse_table_row(line) else {
            continue;
        };
        if is_header_or_separator(&cells) || cells.is_empty() {
            continue;
        }
        rows.push((section, normalize_requirement(&cells[0])));
    }
    rows
}

fn section_from_heading(line: &str) -> Option<Section> {
    let trimmed = line.trim();
    if !trimmed.starts_with("## ") {
        return None;
    }
    let lower = trimmed.trim_start_matches('#').trim().to_ascii_lowercase();
    if lower.starts_with("run metadata") {
        Some(Section::Metadata)
    } else if lower.starts_with("p0 ") {
        Some(Section::P0)
    } else if lower.starts_with("p1 ") {
        Some(Section::P1)
    } else if lower.starts_with("p2 synthetic") {
        Some(Section::P2Synthetic)
    } else if lower.starts_with("p2 real") {
        Some(Section::P2Real)
    } else if lower.starts_with("p3 ") {
        Some(Section::P3)
    } else if lower.starts_with("p4 ") {
        Some(Section::P4)
    } else if lower.starts_with("blockers") {
        Some(Section::Blockers)
    } else {
        Some(Section::Other)
    }
}

fn parse_table_row(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return None;
    }
    Some(
        trimmed
            .trim_matches('|')
            .split('|')
            .map(|cell| cell.trim().to_owned())
            .collect(),
    )
}

fn is_header_or_separator(cells: &[String]) -> bool {
    if cells.is_empty() {
        return true;
    }
    let first = cells[0].trim().to_ascii_lowercase();
    if matches!(first.as_str(), "field" | "required evidence" | "blocker") {
        return true;
    }
    cells.iter().all(|cell| {
        let trimmed = cell.trim();
        !trimmed.is_empty() && trimmed.chars().all(|ch| matches!(ch, '-' | ':' | ' '))
    })
}

fn normalize_status(raw: &str) -> String {
    raw.trim()
        .trim_matches('`')
        .to_ascii_lowercase()
        .replace(' ', "")
}

fn normalize_requirement(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed_template() -> String {
        let mut markdown = EVIDENCE_TEMPLATE.to_owned();
        for field in [
            "protocol",
            "branch",
            "worktree",
            "date",
            "main agent",
            "base commit",
        ] {
            markdown = markdown.replace(&format!("| {field} | |"), &format!("| {field} | x |"));
        }
        markdown.replace("| pending | |", "| done | artifact |")
    }

    #[test]
    fn accepts_complete_template_rows() {
        let markdown = completed_template();
        let (findings, stats) = check_markdown(&markdown, Phase::P0);
        assert!(findings.is_empty(), "{findings:#?}");
        assert!(stats.checked_rows > 1);
        assert_eq!(stats.blocked_rows, 0);
    }

    #[test]
    fn accepts_blocked_with_blocker_row() {
        let mut markdown = completed_template().replacen(
            "| done | artifact |",
            "| blocked | blocked_external_data:dune |",
            1,
        );
        markdown = markdown.replace("| | | |", "| dune unavailable | auth | retry after OAuth |");
        let (findings, stats) = check_markdown(&markdown, Phase::P0);
        assert!(findings.is_empty(), "{findings:#?}");
        assert_eq!(stats.blocked_rows, 1);
        assert_eq!(stats.blocker_rows, 1);
    }

    #[test]
    fn rejects_pending_rows() {
        let markdown = r#"
## Run Metadata
| field | value |
|---|---|
| protocol | curve |

## P1 Authoring Evidence
| required evidence | status | artifact / exact command / summary |
|---|---|---|
| manifest files listed | pending | |
"#;
        let (findings, _) = check_markdown(markdown, Phase::P1);
        assert!(findings.iter().any(|f| f.message.contains("incomplete")));
    }

    #[test]
    fn rejects_done_without_artifact() {
        let markdown = r#"
## Run Metadata
| field | value |
|---|---|
| protocol | curve |

## P4 Land Evidence
| required evidence | status | artifact / exact command / summary |
|---|---|---|
| cargo test output recorded | done | |
"#;
        let (findings, _) = check_markdown(markdown, Phase::P4);
        assert!(findings
            .iter()
            .any(|f| f.message.contains("artifact/summary is empty")));
    }

    #[test]
    fn accepts_na_with_justification() {
        // `n/a` is a valid disposition for rows that explicitly do not apply
        // (e.g. no Codex lane, Dune on a mainnet-only protocol). It counts as a
        // dispositioned row (like blocked) and must carry a justification.
        let markdown = completed_template().replacen(
            "| done | artifact |",
            "| n/a | mainnet-only protocol; Etherscan sweep sufficient |",
            1,
        );
        let (findings, stats) = check_markdown(&markdown, Phase::P0);
        assert!(findings.is_empty(), "{findings:#?}");
        // n/a is settled but NOT a blocker — no Blockers-table row required.
        assert_eq!(stats.blocked_rows, 0);
    }

    #[test]
    fn rejects_na_without_artifact() {
        let markdown = r#"
## Run Metadata
| field | value |
|---|---|
| protocol | curve |

## P2 Real-Tx Evidence
| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Dune MCP/API availability checked | n/a | |
"#;
        let (findings, _) = check_markdown(markdown, Phase::P2);
        assert!(findings
            .iter()
            .any(|f| f.message.contains("n/a but artifact/summary is empty")));
    }

    #[test]
    fn rejects_omitted_template_rows() {
        let markdown = r#"
## Run Metadata
| field | value |
|---|---|
| protocol | curve |
| branch | feat |
| worktree | repo |
| date | today |
| main agent | codex |
| base commit | abc |

## P4 Land Evidence
| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `cargo test --workspace` output recorded | done | ok |
"#;
        let (findings, _) = check_markdown(markdown, Phase::P4);
        assert!(findings
            .iter()
            .any(|f| f.message.contains("missing mandatory row")));
    }

    #[test]
    fn ignores_non_status_tables_in_phase_sections() {
        // A protocol may add an explanatory table (e.g. an L3 dropped-field
        // disposition, a coverage matrix) inside a phase section. `### subsections`
        // do not reset the `## ` section, so such a table sits "inside" P3; it must
        // be IGNORED, not validated as status rows.
        let base = completed_template();
        let analysis = "### L3 — dropped-field disposition\n\
| dropped field | scope verdict | rationale |\n\
|---|---|---|\n\
| appData | enrichment-only | off-chain hash, not statically decodable |\n\
| kind | not a scope boundary | worst-case spend already captured |\n\n";
        let with_analysis = base.replace(
            "## P4 Land Evidence",
            &format!("{analysis}## P4 Land Evidence"),
        );
        assert_ne!(base, with_analysis, "analysis table was not inserted");

        let (f_base, s_base) = check_markdown(&base, Phase::All);
        let (f_with, s_with) = check_markdown(&with_analysis, Phase::All);

        assert_eq!(
            s_base.checked_rows, s_with.checked_rows,
            "an explanatory table must not add mandatory status rows"
        );
        assert_eq!(
            f_base.len(),
            f_with.len(),
            "an explanatory table must not change findings: {f_with:#?}"
        );
        assert!(
            !f_with
                .iter()
                .any(|f| f.message.contains("unsupported status")),
            "explanatory-table cells must not be parsed as statuses: {f_with:#?}"
        );
    }
}
