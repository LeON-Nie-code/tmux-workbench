use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn truncate(value: &str, width: usize) -> String {
    let mut output = String::new();
    for ch in value.chars().take(width) {
        output.push(ch);
    }
    output
}

pub fn sanitize_filename(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub fn edit_note(workspace_id: &str, current_note: &str) -> Result<Option<String>> {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ws-note-{}-{}.md",
        std::process::id(),
        sanitize_filename(workspace_id)
    ));
    {
        let mut file = fs::File::create(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        file.write_all(current_note.as_bytes())?;
    }

    if let Err(err) = edit_file(&path) {
        let _ = fs::remove_file(&path);
        return Err(err);
    }

    let next =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let _ = fs::remove_file(&path);
    if next == current_note {
        Ok(None)
    } else {
        Ok(Some(next))
    }
}

pub fn edit_file(path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let command = format!("{} {}", editor, shell_quote(&path.to_string_lossy()));
    let status = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to run editor")?;
    if !status.success() {
        bail!("editor exited with status {status}");
    }
    Ok(())
}

#[allow(dead_code)]
pub fn path_buf(path: impl Into<PathBuf>) -> PathBuf {
    path.into()
}

#[cfg(test)]
mod tests {
    use super::sanitize_filename;

    #[test]
    fn sanitizes_workspace_id_for_temp_filename() {
        assert_eq!(
            sanitize_filename("AI-Teacher-Baidu/NeuroPlay"),
            "AI-Teacher-Baidu_NeuroPlay"
        );
    }
}
