//! The editor fallback (M4-W12 / T6).
//!
//! The reference extension opens diffs in the VS Code editor
//! (`src/utils.ts:418-490`). A terminal has no editor of its own, so W12 keeps
//! the in-terminal renderer as the primary path and adds an **opt-in**
//! side-channel: when `$GGT_EDITOR` names an editor, `e` on a diff row stages
//! the two revisions into temp files and launches `<editor> -d <old> <new>`.
//!
//! The launch is deliberately split from the argv so a unit test can assert the
//! exact command line without ever spawning a process. `-d` is the diff mode
//! `nvim`/`vim` share; the argv builder is the one place that knows that.

use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

/// The environment variable that turns the fallback on.
pub const ENV_VAR: &str = "GGT_EDITOR";

/// A staged editor invocation: the program, its argv, and the two temp files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPlan {
    /// The editor binary (`nvim`).
    pub program: String,
    /// The arguments after the program (`-d`, old, new).
    pub args: Vec<String>,
    /// The staged "before" file.
    pub old_file: PathBuf,
    /// The staged "after" file.
    pub new_file: PathBuf,
}

impl EditorPlan {
    /// The full argv, program first.
    pub fn argv(&self) -> Vec<String> {
        let mut argv = Vec::with_capacity(self.args.len() + 1);
        argv.push(self.program.clone());
        argv.extend(self.args.iter().cloned());
        argv
    }

    /// The one-line command for the status bar / pager title.
    pub fn command_line(&self) -> String {
        self.argv().join(" ")
    }
}

/// The configured editor, or `None` when the fallback is off.
///
/// An empty or whitespace-only value counts as "off" so an exported-but-blank
/// variable cannot produce a broken `" -d ..."` command line.
pub fn program_from_env() -> Option<String> {
    std::env::var(ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Build the argv for `program` diffing two files.
///
/// Pure — no filesystem access — so it is safe to assert in a test.
pub fn plan_argv(program: &str, old_file: &str, new_file: &str) -> Vec<String> {
    vec![
        program.to_string(),
        "-d".to_string(),
        old_file.to_string(),
        new_file.to_string(),
    ]
}

/// Write `old_content`/`new_content` to temp files and build the plan.
///
/// The files land in the OS temp directory under a `ggt-diff-` prefix so a
/// crash leaves nothing inside the repository. `label` (a file path) is folded
/// into the names to make them recognisable in the editor's tab line.
pub fn stage(
    program: &str,
    label: &str,
    old_content: &str,
    new_content: &str,
) -> io::Result<EditorPlan> {
    let safe: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let pid = std::process::id();
    let dir = std::env::temp_dir();
    let old_file = dir.join(format!("ggt-diff-{pid}-{safe}.old"));
    let new_file = dir.join(format!("ggt-diff-{pid}-{safe}.new"));
    std::fs::write(&old_file, old_content)?;
    std::fs::write(&new_file, new_content)?;
    let argv = plan_argv(
        program,
        &old_file.to_string_lossy(),
        &new_file.to_string_lossy(),
    );
    Ok(EditorPlan {
        program: argv[0].clone(),
        args: argv[1..].to_vec(),
        old_file,
        new_file,
    })
}

/// Run the staged plan, inheriting the terminal.
///
/// The caller is responsible for leaving the alternate screen and raw mode
/// first (see `main::run_editor`); this function only spawns and waits.
pub fn run(plan: &EditorPlan) -> io::Result<ExitStatus> {
    Command::new(&plan.program).args(&plan.args).status()
}

/// Remove the staged temp files, ignoring "already gone".
pub fn cleanup(plan: &EditorPlan) {
    let _ = std::fs::remove_file(&plan.old_file);
    let _ = std::fs::remove_file(&plan.new_file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_argv_is_editor_dash_d_then_the_two_files() {
        let argv = plan_argv("nvim", "/tmp/a.old", "/tmp/a.new");
        assert_eq!(argv, vec!["nvim", "-d", "/tmp/a.old", "/tmp/a.new"]);
    }

    #[test]
    fn the_plan_reports_its_full_command_line() {
        let plan = EditorPlan {
            program: "nvim".into(),
            args: vec!["-d".into(), "/tmp/a.old".into(), "/tmp/a.new".into()],
            old_file: "/tmp/a.old".into(),
            new_file: "/tmp/a.new".into(),
        };
        assert_eq!(plan.command_line(), "nvim -d /tmp/a.old /tmp/a.new");
    }

    #[test]
    fn staging_writes_both_sides_and_cleans_up() {
        let plan = stage("nvim", "src/lib.rs", "old\n", "new\n").unwrap();
        assert_eq!(std::fs::read_to_string(&plan.old_file).unwrap(), "old\n");
        assert_eq!(std::fs::read_to_string(&plan.new_file).unwrap(), "new\n");
        assert_eq!(plan.program, "nvim");
        assert_eq!(plan.args[0], "-d");
        cleanup(&plan);
        assert!(!plan.old_file.exists());
        assert!(!plan.new_file.exists());
    }

    #[test]
    fn the_temp_name_sanitises_the_label() {
        let plan = stage("nvim", "src/a b/c.rs", "", "").unwrap();
        let name = plan
            .old_file
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(!name.contains(' ') && !name.contains('/'), "got {name}");
        cleanup(&plan);
    }
}
