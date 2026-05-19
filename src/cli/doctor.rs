use colored::Colorize;
use serde_json::json;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

impl CheckStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }

    fn icon(self) -> colored::ColoredString {
        match self {
            Self::Ok => "✓".bright_green(),
            Self::Warn => "⚠".yellow(),
            Self::Fail => "✗".bright_red(),
        }
    }
}

#[derive(Debug)]
struct DoctorCheck {
    status: CheckStatus,
    name: &'static str,
    detail: String,
}

impl DoctorCheck {
    fn ok(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Ok,
            name,
            detail: detail.into(),
        }
    }

    fn warn(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Warn,
            name,
            detail: detail.into(),
        }
    }

    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Fail,
            name,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Default)]
struct CargoMetadata {
    workspace_root: Option<PathBuf>,
    target_directory: Option<PathBuf>,
}

#[derive(Debug)]
struct CargoConfig {
    path: PathBuf,
    text: Option<String>,
}

impl CargoConfig {
    fn load(workspace_root: &Path) -> Self {
        let path = workspace_root.join(".cargo").join("config.toml");
        let text = fs::read_to_string(&path).ok();
        Self { path, text }
    }

    fn value(&self, key: &str) -> Option<String> {
        self.text
            .as_deref()
            .and_then(|text| find_config_value(text, key))
    }
}

pub fn cmd_doctor(json_output: bool) {
    let metadata = cargo_metadata();
    let workspace_root = metadata
        .workspace_root
        .clone()
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let cargo_config = CargoConfig::load(&workspace_root);

    let mut checks = vec![
        DoctorCheck::ok("anvil", env!("CARGO_PKG_VERSION")),
        check_current_exe(),
        check_command("rustc", &["--version"], true),
        check_command("cargo", &["--version"], true),
        check_workspace(&workspace_root),
        check_target_dir(metadata.target_directory.as_deref()),
    ];
    checks.extend(check_cargo_limits(&cargo_config));
    checks.extend(check_z3(&cargo_config));
    checks.extend(check_sqlite(&cargo_config));
    checks.push(check_memory());

    if json_output {
        print_json(&checks);
    } else {
        print_human(&checks);
    }

    if checks.iter().any(|check| check.status == CheckStatus::Fail) {
        std::process::exit(1);
    }
}

fn cargo_metadata() -> CargoMetadata {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output();

    let Ok(output) = output else {
        return CargoMetadata::default();
    };

    if !output.status.success() {
        return CargoMetadata::default();
    }

    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return CargoMetadata::default();
    };

    CargoMetadata {
        workspace_root: value["workspace_root"].as_str().map(PathBuf::from),
        target_directory: value["target_directory"].as_str().map(PathBuf::from),
    }
}

fn check_current_exe() -> DoctorCheck {
    match env::current_exe() {
        Ok(path) => DoctorCheck::ok("binary", path.display().to_string()),
        Err(e) => DoctorCheck::warn("binary", format!("Cannot resolve current executable: {e}")),
    }
}

fn check_command(name: &'static str, args: &[&str], critical: bool) -> DoctorCheck {
    match command_stdout(name, args, None) {
        Ok(output) => DoctorCheck::ok(name, output),
        Err(e) if critical => DoctorCheck::fail(name, e),
        Err(e) => DoctorCheck::warn(name, e),
    }
}

fn check_workspace(workspace_root: &Path) -> DoctorCheck {
    let manifest = workspace_root.join("Cargo.toml");
    if manifest.exists() {
        DoctorCheck::ok("workspace", workspace_root.display().to_string())
    } else {
        DoctorCheck::warn(
            "workspace",
            format!("Cargo.toml not found at {}", manifest.display()),
        )
    }
}

fn check_target_dir(target_dir: Option<&Path>) -> DoctorCheck {
    match target_dir {
        Some(path) if path.exists() => DoctorCheck::ok("cargo target", path.display().to_string()),
        Some(path) => DoctorCheck::warn(
            "cargo target",
            format!("{} does not exist yet", path.display()),
        ),
        None => DoctorCheck::warn(
            "cargo target",
            "cargo metadata did not return target_directory",
        ),
    }
}

fn check_cargo_limits(cargo_config: &CargoConfig) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    let config_jobs = cargo_config.value("jobs");
    let config_target_dir = cargo_config.value("target-dir");

    let jobs_detail = match (env::var("CARGO_BUILD_JOBS").ok(), config_jobs) {
        (Some(value), _) => format!("CARGO_BUILD_JOBS={value}"),
        (None, Some(value)) => format!("jobs={value} in {}", cargo_config.path.display()),
        (None, None) => "No explicit job limit found".to_string(),
    };
    if jobs_detail.contains("No explicit") {
        checks.push(DoctorCheck::warn("cargo jobs", jobs_detail));
    } else {
        checks.push(DoctorCheck::ok("cargo jobs", jobs_detail));
    }

    match config_target_dir {
        Some(value) => checks.push(DoctorCheck::ok("target-dir config", value)),
        None => checks.push(DoctorCheck::warn(
            "target-dir config",
            format!("No target-dir found in {}", cargo_config.path.display()),
        )),
    }

    let incremental = env::var("CARGO_INCREMENTAL").unwrap_or_else(|_| "unset".to_string());
    let dev_debug = env::var("CARGO_PROFILE_DEV_DEBUG").unwrap_or_else(|_| "unset".to_string());
    checks.push(DoctorCheck::ok(
        "cargo env",
        format!("CARGO_INCREMENTAL={incremental}, CARGO_PROFILE_DEV_DEBUG={dev_debug}"),
    ));

    checks
}

fn check_z3(cargo_config: &CargoConfig) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    checks.push(check_command("z3", &["--version"], false));

    let header = path_from_env_or_config(
        "Z3_SYS_Z3_HEADER",
        cargo_config,
        "/opt/homebrew/Cellar/z3/4.15.4/include/z3.h",
    );
    if header.exists() {
        checks.push(DoctorCheck::ok("z3 header", header.display().to_string()));
    } else {
        checks.push(DoctorCheck::warn(
            "z3 header",
            format!("{} not found", header.display()),
        ));
    }

    let lib_dirs = path_list_from_env_or_config(
        "LIBRARY_PATH",
        cargo_config,
        "/opt/homebrew/Cellar/z3/4.15.4/lib",
    );
    if let Some(lib_dir) = find_library_dir(&lib_dirs, &["libz3"]) {
        checks.push(DoctorCheck::ok("z3 lib", lib_dir.display().to_string()));
    } else {
        checks.push(DoctorCheck::warn(
            "z3 lib",
            format!("libz3 not found in {}", display_path_list(&lib_dirs)),
        ));
    }

    checks
}

fn check_sqlite(cargo_config: &CargoConfig) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    let lib_dir = path_from_env_or_config(
        "SQLITE3_LIB_DIR",
        cargo_config,
        "/opt/homebrew/opt/sqlite/lib",
    );
    if lib_dir.exists() {
        checks.push(DoctorCheck::ok("sqlite lib", lib_dir.display().to_string()));
    } else {
        checks.push(DoctorCheck::warn(
            "sqlite lib",
            format!("{} not found", lib_dir.display()),
        ));
    }

    let include_dir = path_from_env_or_config(
        "SQLITE3_INCLUDE_DIR",
        cargo_config,
        "/opt/homebrew/opt/sqlite/include",
    );
    if include_dir.exists() {
        checks.push(DoctorCheck::ok(
            "sqlite include",
            include_dir.display().to_string(),
        ));
    } else {
        checks.push(DoctorCheck::warn(
            "sqlite include",
            format!("{} not found", include_dir.display()),
        ));
    }

    checks
}

fn check_memory() -> DoctorCheck {
    if cfg!(target_os = "macos") {
        match command_stdout("sysctl", &["-n", "hw.memsize"], None)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
        {
            Some(bytes) => DoctorCheck::ok("memory", format_bytes(bytes)),
            None => DoctorCheck::warn("memory", "Could not read hw.memsize"),
        }
    } else if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        let total_kb = meminfo
            .lines()
            .find_map(|line| line.strip_prefix("MemTotal:"))
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok());
        match total_kb {
            Some(kb) => DoctorCheck::ok("memory", format_bytes(kb * 1024)),
            None => DoctorCheck::warn("memory", "Could not parse /proc/meminfo"),
        }
    } else {
        DoctorCheck::warn("memory", "No memory probe for this platform")
    }
}

fn command_stdout(cmd: &str, args: &[&str], cwd: Option<&Path>) -> Result<String, String> {
    let mut command = Command::new(cmd);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let output = command
        .output()
        .map_err(|e| format!("{cmd} unavailable: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(if stdout.is_empty() { stderr } else { stdout })
    } else {
        Err(if stderr.is_empty() {
            format!("{cmd} exited with {}", output.status)
        } else {
            stderr
        })
    }
}

fn path_from_env_or_config(key: &str, cargo_config: &CargoConfig, fallback: &str) -> PathBuf {
    env::var_os(key)
        .or_else(|| cargo_config.value(key).map(OsString::from))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback))
}

fn path_list_from_env_or_config(
    key: &str,
    cargo_config: &CargoConfig,
    fallback: &str,
) -> Vec<PathBuf> {
    let value = env::var_os(key)
        .or_else(|| cargo_config.value(key).map(OsString::from))
        .unwrap_or_else(|| OsString::from(fallback));
    env::split_paths(&value).collect()
}

fn find_library_dir<'a>(dirs: &'a [PathBuf], prefixes: &[&str]) -> Option<&'a PathBuf> {
    dirs.iter().find(|dir| {
        if !dir.is_dir() {
            return false;
        }

        fs::read_dir(dir).is_ok_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                prefixes.iter().any(|prefix| name.starts_with(prefix))
            })
        })
    })
}

fn display_path_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
}

fn find_config_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        let value = trimmed.strip_prefix(key)?.trim_start();
        let value = value.strip_prefix('=')?.trim();
        Some(value.trim_matches('"').to_string())
    })
}

fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    format!("{:.1} GiB", bytes as f64 / GIB)
}

fn print_human(checks: &[DoctorCheck]) {
    eprintln!("  {}", "ANVIL DOCTOR".bold());
    eprintln!();

    for check in checks {
        eprintln!(
            "  {} {:<18} {}",
            check.status.icon(),
            check.name.bold(),
            check.detail
        );
    }

    let ok = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Ok)
        .count();
    let warn = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Warn)
        .count();
    let fail = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Fail)
        .count();

    eprintln!();
    eprintln!(
        "  {} {} ok, {} warnings, {} failures",
        if fail == 0 {
            "✓".bright_green()
        } else {
            "✗".bright_red()
        },
        ok,
        warn,
        fail
    );
}

fn print_json(checks: &[DoctorCheck]) {
    let checks_json: Vec<serde_json::Value> = checks
        .iter()
        .map(|check| {
            json!({
                "status": check.status.as_str(),
                "name": check.name,
                "detail": check.detail,
            })
        })
        .collect();

    let fail = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Fail)
        .count();
    println!(
        "{}",
        json!({
            "ok": fail == 0,
            "checks": checks_json,
        })
    );
}
