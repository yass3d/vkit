use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

const FAILED: i32 = 1;

pub fn run_if_asked() -> Option<i32> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = arguments.split_first()?;
    match command.as_str() {
        "measure-import" => Some(attached(|| measure_import(rest))),
        "--help" | "-h" | "help" => Some(attached(|| {
            print!("{}", usage());
            0
        })),
        "--version" | "-V" => Some(attached(|| {
            println!("{} {}", crate::APP_NAME, env!("CARGO_PKG_VERSION"));
            0
        })),
        _ => None,
    }
}

fn usage() -> String {
    format!(
        "{} {}\n\n\
         Usage:\n  \
         Vkit                                 open the editor\n  \
         Vkit measure-import <file> [--whole] time each stage of a mesh import\n  \
         Vkit --version\n  \
         Vkit --help\n\n\
         measure-import options:\n  \
         --whole    load the mesh at full resolution and skip the rebuild,\n             \
         reporting what the editor would be asked to carry\n",
        crate::APP_NAME,
        env!("CARGO_PKG_VERSION")
    )
}

fn measure_import(arguments: &[String]) -> i32 {
    let mut path: Option<&str> = None;
    let mut whole = false;
    for argument in arguments {
        match argument.as_str() {
            "--whole" => whole = true,
            flag if flag.starts_with('-') => {
                eprintln!("measure-import: unknown option {flag}");
                return FAILED;
            }
            value if path.is_none() => path = Some(value),
            extra => {
                eprintln!("measure-import: unexpected argument {extra}");
                return FAILED;
            }
        }
    }
    let Some(path) = path.map(Path::new) else {
        eprintln!("measure-import: a mesh file is required\n\n{}", usage());
        return FAILED;
    };
    if !path.is_file() {
        eprintln!("measure-import: {} is not a file", path.display());
        return FAILED;
    }

    let bytes = path.metadata().map(|meta| meta.len()).unwrap_or_default();
    println!("{}", path.display());
    println!("  container      {}", mebibytes(bytes));

    let report = if whole {
        measure_whole(path)
    } else {
        measure_pipeline(path)
    };
    match report {
        Ok(report) => {
            print!("{report}");
            0
        }
        Err(error) => {
            eprintln!("measure-import: {error}");
            FAILED
        }
    }
}

fn measure_pipeline(path: &Path) -> Result<String, String> {
    let mut stages: Vec<(&str, f32)> = Vec::new();
    let mut phase_started = Instant::now();
    let mut last_phase = None;
    let whole = Instant::now();
    let prepared = crate::importers::prepare_mesh_import_with_progress(path, |progress| {
        if last_phase != Some(progress.phase) {
            if let Some(phase) = last_phase {
                stages.push((phase_name(phase), phase_started.elapsed().as_secs_f32()));
            }
            phase_started = Instant::now();
            last_phase = Some(progress.phase);
        }
    })
    .map_err(|error| error.to_string())?;
    if let Some(phase) = last_phase {
        stages.push((phase_name(phase), phase_started.elapsed().as_secs_f32()));
    }
    let total = whole.elapsed().as_secs_f32();

    let mut report = String::new();
    let source = prepared
        .source_triangles
        .unwrap_or(prepared.final_triangles);
    let _ = writeln!(report, "  source         {} triangles", thousands(source));
    for (stage, seconds) in stages {
        let _ = writeln!(report, "  {stage:<14} {seconds:>7.2} s");
    }
    let _ = writeln!(report, "  total          {total:>7.2} s");
    let _ = writeln!(
        report,
        "  kept           {} triangles ({:.1}% of source)",
        thousands(prepared.final_triangles),
        prepared.final_triangles as f64 / source.max(1) as f64 * 100.0
    );
    Ok(report)
}

fn measure_whole(path: &Path) -> Result<String, String> {
    let started = Instant::now();
    let census =
        crate::importers::census_at_native_resolution(path).map_err(|error| error.to_string())?;
    let parse = started.elapsed().as_secs_f32();

    let crate::importers::MeshCensus {
        triangles,
        vertices,
    } = census;
    let mut report = String::new();
    let _ = writeln!(
        report,
        "  source         {} triangles",
        thousands(triangles)
    );
    let _ = writeln!(report, "  parse          {parse:>7.2} s");
    let _ = writeln!(report, "  vertices       {}", thousands(vertices));
    let _ = writeln!(
        report,
        "  positions      {} at rest",
        mebibytes((vertices * size_of::<[f64; 3]>()) as u64)
    );
    let _ = writeln!(
        report,
        "  a sculpt pass  {} touched per stroke step",
        thousands(vertices)
    );
    Ok(report)
}

const fn phase_name(phase: crate::importers::MeshImportPhase) -> &'static str {
    match phase {
        crate::importers::MeshImportPhase::MeshLoading => "parse",
        crate::importers::MeshImportPhase::Simplification => "rebuild",
    }
}

fn thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn mebibytes(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

fn attached(body: impl FnOnce() -> i32) -> i32 {
    #[cfg(windows)]
    attach_parent_console();
    body()
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "borrowing the launching shell's console is a Win32 call and has no safe wrapper"
)]
fn attach_parent_console() {
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    unsafe { windows_sys::Win32::System::Console::AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_launch_and_a_dropped_file_both_open_the_window() {
        for arguments in [vec![], vec!["C:/scans/face.glb".to_owned()]] {
            let (command, _) = match arguments.split_first() {
                Some(split) => split,
                None => continue,
            };
            assert!(
                !matches!(command.as_str(), "measure-import" | "--help" | "--version"),
                "a dropped path must not read as a subcommand"
            );
        }
    }

    #[test]
    fn a_missing_file_fails_rather_than_measuring_nothing() {
        let code = measure_import(&["C:/nowhere/absent.glb".to_owned()]);
        assert_eq!(code, FAILED);
        assert_eq!(measure_import(&[]), FAILED, "a path is required");
    }

    #[test]
    fn counts_are_grouped_the_way_someone_reads_them() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(1_900_000), "1,900,000");
    }

    #[test]
    fn usage_lists_every_command_that_exists() {
        let usage = usage();
        for command in ["measure-import", "--whole", "--version", "--help"] {
            assert!(usage.contains(command), "{command} is undocumented");
        }
    }
}
