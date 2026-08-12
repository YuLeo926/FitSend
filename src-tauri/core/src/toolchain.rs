use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

pub fn command(name: &str) -> Command {
    Command::new(resolve(name))
}

pub fn available(name: &str) -> bool {
    command(name)
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn resolve(name: &str) -> PathBuf {
    if let Some(path) = override_path(name).filter(|path| path.is_file()) {
        return path;
    }

    if let Ok(executable) = env::current_exe() {
        if let Some(path) = resolve_near_executable(name, &executable) {
            return path;
        }
    }

    PathBuf::from(tool_filename(name))
}

fn override_path(name: &str) -> Option<PathBuf> {
    let key = match name {
        "ffmpeg" => "FITSEND_FFMPEG_PATH",
        "ffprobe" => "FITSEND_FFPROBE_PATH",
        _ => return None,
    };
    env::var_os(key).map(PathBuf::from)
}

fn resolve_near_executable(name: &str, executable: &Path) -> Option<PathBuf> {
    let parent = executable.parent()?;
    let filename = tool_filename(name);
    [
        parent.join(&filename),
        parent.join("ffmpeg").join(&filename),
        parent.join("resources").join("ffmpeg").join(&filename),
        parent.join("Resources").join("ffmpeg").join(&filename),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

fn tool_filename(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn prefers_a_tool_bundled_next_to_the_application() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("FitSend.exe");
        let bundled_dir = directory.path().join("ffmpeg");
        let bundled = bundled_dir.join(tool_filename("ffmpeg"));
        fs::create_dir_all(&bundled_dir).unwrap();
        fs::write(&bundled, b"test binary").unwrap();

        assert_eq!(
            resolve_near_executable("ffmpeg", &executable),
            Some(bundled)
        );
    }

    #[test]
    fn falls_back_when_no_bundled_tool_exists() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("FitSend.exe");
        assert_eq!(resolve_near_executable("ffprobe", &executable), None);
    }
}
