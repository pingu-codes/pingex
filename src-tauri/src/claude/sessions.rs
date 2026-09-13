//! Session identity comes from Claude's transcript, not our turn journal.

use std::fs;
use std::io::ErrorKind;

use crate::util::host::Host;

pub(crate) fn config_path(host: &Host, path: &str) -> Result<String, String> {
    if path == "~" || path.starts_with("~/") {
        let home = host
            .home_dir()
            .ok_or("Could not resolve the Claude Home on its Host")?;
        Ok(host.join_str(&home, path.strip_prefix("~/").unwrap_or("")))
    } else {
        Ok(path.to_string())
    }
}

pub(crate) fn should_resume(
    host: &Host,
    config_dir: &str,
    thread_id: &str,
    has_history: bool,
) -> Result<bool, String> {
    uuid::Uuid::parse_str(thread_id).map_err(|_| "Invalid Claude session ID".to_string())?;
    let config_dir = config_path(host, config_dir)?;
    let root = host.to_local(&config_dir).join("projects");
    let missing = || {
        if has_history {
            Err(format!("Claude session {thread_id} is missing from {config_dir}. Restore its session file or select the Home that contains it to continue this conversation."))
        } else {
            Ok(false)
        }
    };
    let projects = match fs::read_dir(&root) {
        Ok(projects) => projects,
        Err(error) if error.kind() == ErrorKind::NotFound => return missing(),
        Err(error) => {
            return Err(format!(
                "Could not read Claude sessions in {config_dir}: {error}"
            ))
        }
    };
    let mut found = false;
    for project in projects {
        let project = project.map_err(|error| format!("Could not read Claude project: {error}"))?;
        if !project
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            continue;
        }
        let path = project.path().join(format!("{thread_id}.jsonl"));
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Could not read Claude session {}: {error}",
                    path.display()
                ))
            }
        };
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!(
                "Claude session {} is empty or is not a transcript file",
                path.display()
            ));
        }
        if found {
            return Err(format!(
                "Claude session {thread_id} appears in multiple projects in {config_dir}"
            ));
        }
        found = true;
    }
    if found {
        Ok(true)
    } else {
        missing()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_plan_resumes_even_without_a_completed_turn() {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("projects/old-working-directory");
        fs::create_dir_all(&project).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        fs::write(
            project.join(format!("{id}.jsonl")),
            "{\"type\":\"assistant\"}\n",
        )
        .unwrap();
        for has_history in [false, true] {
            let resume = should_resume(
                &Host::Native,
                home.path().to_str().unwrap(),
                &id,
                has_history,
            )
            .unwrap();
            let args = super::super::driver::turn_args(None, None, "plan", resume, &id);
            assert_eq!(&args[..2], &["--resume", id.as_str()]);
        }
        let other = tempfile::tempdir().unwrap();
        assert!(
            should_resume(&Host::Native, other.path().to_str().unwrap(), &id, true)
                .unwrap_err()
                .contains("missing")
        );
        assert!(!should_resume(&Host::Native, other.path().to_str().unwrap(), &id, false).unwrap());
    }

    #[test]
    fn empty_or_ambiguous_transcripts_never_start_a_new_session() {
        let home = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let check = || should_resume(&Host::Native, home.path().to_str().unwrap(), &id, false);
        for name in ["a", "b"] {
            let project = home.path().join("projects").join(name);
            fs::create_dir_all(&project).unwrap();
            let file = project.join(format!("{id}.jsonl"));
            fs::write(&file, "").unwrap();
            assert!(check().is_err());
            fs::write(file, "{}\n").unwrap();
        }
        assert!(check().unwrap_err().contains("multiple"));
    }
}
