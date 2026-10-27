use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};
thread_local! {static CANCELLATION:RefCell<Option<Arc<AtomicBool>>>=const{RefCell::new(None)};}
pub type Progress = Arc<std::sync::Mutex<String>>;
thread_local! {static PROGRESS: RefCell<Option<Progress>> = const { RefCell::new(None) };}
pub fn set_progress(progress: Progress) {
    PROGRESS.with(|slot| *slot.borrow_mut() = Some(progress));
}
fn read_progress(
    mut reader: impl Read,
    progress: Option<Progress>,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    let mut line = Vec::new();
    let mut buffer = [0u8; 2048];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let keep = count.min((2 * 1024 * 1024usize).saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
        for byte in &buffer[..count] {
            if *byte == b'\r' || *byte == b'\n' {
                if !line.is_empty() {
                    if let Some(target) = &progress
                        && let Ok(mut value) = target.lock()
                    {
                        *value = String::from_utf8_lossy(&line)
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(240)
                            .collect();
                    }
                    line.clear();
                }
            } else if line.len() < 2048 {
                line.push(*byte);
            }
        }
    }
    Ok((bytes, truncated))
}
pub fn set_cancellation(token: Arc<AtomicBool>) {
    CANCELLATION.with(|c| *c.borrow_mut() = Some(token));
}
pub fn is_cancelled() -> bool {
    CANCELLATION.with(|c| {
        c.borrow()
            .as_ref()
            .is_some_and(|t| t.load(Ordering::Relaxed))
    })
}
pub fn reset_cancellation() {
    CANCELLATION.with(|c| {
        if let Some(token) = c.borrow().as_ref() {
            token.store(false, Ordering::Relaxed);
        }
    });
}
fn read_bounded(mut reader: impl Read, limit: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let keep = count.min(limit.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    Ok((bytes, truncated))
}

pub type Result<T> = std::result::Result<T, String>;

static COMMANDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn command_count() -> u64 {
    COMMANDS.load(Ordering::Relaxed)
}
pub fn find_executable(program: &str) -> Option<std::path::PathBuf> {
    let name = if cfg!(windows) {
        format!("{program}.exe")
    } else {
        program.into()
    };
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        candidates.push(parent.join("tools").join(&name));
    }
    if let Some(paths) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&paths).map(|path| path.join(&name)));
    }
    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
        let programs = std::env::var_os("ProgramFiles").map(std::path::PathBuf::from);
        for prefix in programs
            .iter()
            .cloned()
            .chain(local.iter().map(|path| path.join("Programs")))
        {
            if program == "git" {
                candidates.push(prefix.join("Git/cmd/git.exe"));
            }
            if program == "gh" {
                candidates.push(prefix.join("GitHub CLI/gh.exe"));
            }
        }
        if program == "git"
            && let Some(local) = local
            && let Ok(entries) = std::fs::read_dir(local.join("GitHubDesktop"))
        {
            let mut paths: Vec<_> = entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("app-"))
                .map(|entry| entry.path().join("resources/app/git/cmd/git.exe"))
                .collect();
            paths.sort();
            paths.reverse();
            candidates.extend(paths);
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}
pub fn command(program: &str, directory: &Path) -> Command {
    COMMANDS.fetch_add(1, Ordering::Relaxed);
    let resolved = if program == "git" || program == "gh" {
        find_executable(program)
    } else {
        None
    };
    let mut command = Command::new(resolved.as_deref().unwrap_or_else(|| Path::new(program)));
    if program == "gh"
        && let Some(git) = find_executable("git")
        && let Some(parent) = git.parent()
    {
        let mut paths = vec![parent.to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(path) = std::env::join_paths(paths) {
            command.env("PATH", path);
        }
    }
    command.current_dir(directory);
    if program == "git" {
        command.arg("--no-pager");
    }
    command.env("GIT_OPTIONAL_LOCKS", "0");
    if program == "git"
        && let Some(path) = crate::settings::isolated_git_config()
    {
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", path);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

pub fn run(program: &str, directory: &Path, args: &[String]) -> Result<String> {
    capture(program, directory, args, true)
}

pub fn stdout(program: &str, directory: &Path, args: &[String]) -> Result<String> {
    capture(program, directory, args, false)
}

fn capture(program: &str, directory: &Path, args: &[String], diagnostics: bool) -> Result<String> {
    if is_cancelled() {
        return Err("Operation canceled. Refresh the repository to inspect its state.".into());
    }
    if (program == "git" && !crate::safety::read_only_git(args))
        || (program == "gh" && !crate::safety::read_only_gh(args))
    {
        crate::safety::ensure_writable(directory)?;
    }
    if program == "git" && args.first().is_some_and(|a| a == "commit") {
        crate::commit_lock::guard_commit(directory, args)?;
    }
    if program == "git" && args.first().is_some_and(|a| a == "add") {
        crate::commit_lock::guard_add(directory, args)?;
    }
    let network = program == "git"
        && args
            .first()
            .is_some_and(|a| matches!(a.as_str(), "fetch" | "push" | "pull" | "clone"));
    let progress = if network {
        PROGRESS.with(|slot| slot.borrow().clone())
    } else {
        None
    };
    if let Some(target) = &progress
        && let Ok(mut value) = target.lock()
    {
        *value = "Connecting to remote…".into();
    }
    let mut invocation = command(program, directory);
    if program == "git"
        && args.iter().any(|a| a == "--")
        && args.first().is_some_and(|a| {
            matches!(
                a.as_str(),
                "add" | "restore" | "rm" | "diff" | "show" | "status" | "blame" | "ls-files"
            )
        })
    {
        invocation.arg("--literal-pathspecs");
    }
    if program == "git"
        && args
            .first()
            .is_some_and(|a| matches!(a.as_str(), "diff" | "show" | "log"))
    {
        invocation
            .arg(&args[0])
            .args(["--no-ext-diff", "--no-textconv"])
            .args(&args[1..]);
    } else if network && !args.iter().any(|a| a == "--progress") {
        invocation.arg(&args[0]).arg("--progress").args(&args[1..]);
    } else {
        invocation.args(args);
    }
    let output = collect(invocation, program, directory, diagnostics, progress, None)?;
    decode(output, program, args, diagnostics)
}

struct Captured {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: bool,
}
fn collect(
    mut invocation: Command,
    program: &str,
    _directory: &Path,
    diagnostics: bool,
    progress: Option<Progress>,
    input: Option<&[u8]>,
) -> Result<Captured> {
    if is_cancelled() {
        return Err("Operation canceled before the next command.".into());
    }
    let mut child = invocation
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("GH_PROMPT_DISABLED", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!("Cannot start {program}: {e}. Check that it is installed and on PATH.")
        })?;
    let writer = if let Some(bytes) = input {
        let mut stdin = child.stdin.take().ok_or("Cannot open command input.")?;
        let bytes = bytes.to_vec();
        Some(std::thread::spawn(move || stdin.write_all(&bytes)))
    } else {
        None
    };
    let stdout = child.stdout.take().ok_or("Cannot read command output.")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Cannot read command diagnostics.")?;
    let limit = if diagnostics {
        8 * 1024 * 1024
    } else {
        64 * 1024 * 1024
    };
    let output_reader = std::thread::spawn(move || read_bounded(stdout, limit));
    let error_reader = std::thread::spawn(move || read_progress(stderr, progress));
    let started = std::time::Instant::now();
    let status = loop {
        if is_cancelled() {
            #[cfg(windows)]
            {
                let _ = command("taskkill", _directory)
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .output();
            }
            #[cfg(unix)]
            // command() creates a child-owned process group, separate from the app.
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            let _ = output_reader.join();
            let _ = error_reader.join();
            if let Some(writer) = writer {
                let _ = writer.join();
            }
            return Err(
                "Operation canceled. Refresh and review repository status before continuing."
                    .into(),
            );
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
        std::thread::sleep(if started.elapsed() < Duration::from_millis(100) {
            Duration::from_millis(5)
        } else {
            Duration::from_millis(25)
        });
    };
    let (stdout, truncated) = output_reader
        .join()
        .map_err(|_| "Output reader stopped.")?
        .map_err(|e| e.to_string())?;
    let (stderr, _) = error_reader
        .join()
        .map_err(|_| "Diagnostic reader stopped.")?
        .map_err(|e| e.to_string())?;
    if let Some(writer) = writer {
        let written = writer.join().map_err(|_| "Command input writer stopped.")?;
        if status.success() {
            written.map_err(|e| e.to_string())?;
        }
    }
    if truncated && !diagnostics {
        return Err(
            "This result exceeds the 64 MiB inspection limit. Narrow the file or commit range."
                .into(),
        );
    }
    Ok(Captured {
        status,
        stdout,
        stderr,
        truncated,
    })
}
fn decode(output: Captured, program: &str, args: &[String], diagnostics: bool) -> Result<String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let difference = program == "git"
        && args.first().is_some_and(|s| s == "diff")
        && args.iter().any(|s| s == "--no-index")
        && output.status.code() == Some(1);
    if output.status.success() || difference {
        if diagnostics {
            Ok(format!(
                "{stdout}{stderr}{}",
                if output.truncated {
                    "\n[Display output limited to 8 MiB. Narrow the selection for a complete result.]"
                } else {
                    ""
                }
            ))
        } else {
            Ok(stdout.into_owned())
        }
    } else {
        Err(format!(
            "{program} exited with {}\n{stdout}{stderr}",
            output.status
        ))
    }
}

pub fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

pub fn reveal(path: &Path) -> Result<String> {
    #[cfg(windows)]
    let mut app = Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut app = Command::new("open");
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let mut app = Command::new("xdg-open");
    app.arg(if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    })
    .spawn()
    .map_err(|e| e.to_string())?;
    Ok("Opened the containing folder.".into())
}

pub fn terminal(path: &Path) -> Result<String> {
    crate::safety::ensure_writable(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("cmd.exe")
            .arg("/K")
            .current_dir(path)
            .creation_flags(0x00000010)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal"])
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        Command::new("x-terminal-emulator")
            .current_dir(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok("Opened a terminal in the repository.".into())
}

pub fn input(
    program: &str,
    directory: &Path,
    args: &[String],
    bytes: &[u8],
    environment: &[(String, String)],
) -> Result<String> {
    if program != "git" || !crate::safety::read_only_git(args) {
        crate::safety::ensure_writable(directory)?;
    }
    if is_cancelled() {
        return Err("Operation canceled before the next write.".into());
    }
    if program == "git" && args.first().is_some_and(|s| s == "commit") {
        crate::commit_lock::guard_commit(directory, args)?;
    }
    let mut invocation = command(program, directory);
    invocation.args(args).envs(environment.iter().cloned());
    let output = collect(invocation, program, directory, false, None, Some(bytes))?;
    decode(output, program, args, false)
}

/// Binary inspection must not pass through UTF-8 conversion.
pub fn git_bytes(directory: &Path, args: &[String]) -> Result<Vec<u8>> {
    if !crate::safety::read_only_git(args) {
        return Err("Binary inspection accepts read-only Git commands.".into());
    }
    let mut invocation = command("git", directory);
    invocation.args(args);
    let output = collect(invocation, "git", directory, false, None, None)?;
    if !output.status.success() {
        return Err(format!(
            "Git inspection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

/// Device login needs a terminal where the user can respond to prompts.
pub fn github_login() -> Result<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let executable = find_executable("gh").ok_or_else(|| {
            "GitHub CLI was not found. Install it, then select Check installation again.".to_owned()
        })?;
        let mut invocation = Command::new(&executable);
        invocation.args(["auth", "login", "--web"]);
        if let Some(git_dir) =
            find_executable("git").and_then(|git| git.parent().map(Path::to_path_buf))
        {
            let mut paths = vec![git_dir];
            if let Some(existing) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&existing));
            }
            if let Ok(path) = std::env::join_paths(paths) {
                invocation.env("PATH", path);
            }
        }
        invocation.creation_flags(0x00000010).spawn().map_err(|e| {
            format!(
                "Cannot open GitHub sign-in using {}: {e}",
                executable.display()
            )
        })?;
        Ok("Complete sign-in in the new console and browser, then select Check account.".into())
    }
    #[cfg(not(windows))]
    {
        Err("Run `gh auth login --web` in your terminal, then check the account here.".into())
    }
}
