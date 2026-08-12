use std::process::ExitCode;

enum ExportMode {
    Write,
    Check,
}

/// Runs the requested xtask command from the workspace root.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Parses the xtask command line and dispatches to the matching workflow.
fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let Some(command) = arguments.next() else {
        return Err(usage());
    };
    let mode = parse_export_mode(arguments.next())?;
    if let Some(unexpected) = arguments.next() {
        return Err(format!("unexpected argument `{unexpected}`\n{}", usage()));
    }

    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "failed to determine workspace root".to_string())?;

    match command.as_str() {
        "export-contracts" => match mode {
            ExportMode::Write => xtask::run_export_contracts(workspace_root)
                .map_err(|error| format!("failed to export contracts: {error}")),
            ExportMode::Check => xtask::check_exported_contracts(workspace_root)
                .map_err(|error| format!("generated contracts are stale: {error}")),
        },
        "export-plugin-sdk" => match mode {
            ExportMode::Write => xtask::run_export_plugin_sdk(workspace_root)
                .map_err(|error| format!("failed to export plugin SDK: {error}")),
            ExportMode::Check => xtask::check_exported_plugin_sdk(workspace_root)
                .map_err(|error| format!("generated plugin SDK is stale: {error}")),
        },
        _ => Err(format!("unknown xtask command `{command}`\n{}", usage())),
    }
}

/// Parses the optional read-only generation check without overloading a boolean flag.
fn parse_export_mode(argument: Option<String>) -> Result<ExportMode, String> {
    match argument.as_deref() {
        None => Ok(ExportMode::Write),
        Some("--check") => Ok(ExportMode::Check),
        Some(unexpected) => Err(format!("unexpected argument `{unexpected}`\n{}", usage())),
    }
}

/// Returns the stable command-line contract shown for every invocation error.
fn usage() -> String {
    "usage: cargo xtask <export-contracts|export-plugin-sdk> [--check]".to_owned()
}
