use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub fn command(name: &str) -> Command {
    let mut command = Command::new(resolve(name));
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Media tools are background workers, not interactive console programs.
        // Prevent console flashes and keep console events from interrupting them.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
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

    #[cfg(windows)]
    #[test]
    fn background_commands_do_not_attach_or_create_a_console() {
        // Check the real child process, not just the requested creation flags.
        // A console close or Ctrl+C must not interrupt the GUI's media tools.
        if env::var_os("FITSEND_TEST_CONSOLE_CHILD").is_none() {
            // Start a hidden console host even if cargo itself has no console.
            let output = Command::new("powershell")
                .env("FITSEND_TEST_CONSOLE_CHILD", "1")
                .env("FITSEND_TEST_EXECUTABLE", env::current_exe().unwrap())
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "$ErrorActionPreference = 'Stop'; $child = Start-Process -FilePath $env:FITSEND_TEST_EXECUTABLE -ArgumentList '--exact','toolchain::tests::background_commands_do_not_attach_or_create_a_console' -WindowStyle Hidden -Wait -PassThru; exit $child.ExitCode",
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "console-hosted test failed: {output:?}"
            );
            return;
        }
        let output = command("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$ErrorActionPreference = 'Stop'; Add-Type -Namespace FitSendTest -Name Console -MemberDefinition '[DllImport(\"kernel32.dll\")] public static extern IntPtr GetConsoleWindow();'; if ([FitSendTest.Console]::GetConsoleWindow() -ne [IntPtr]::Zero) { exit 91 }",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "child had a console: {output:?}");
    }
}
