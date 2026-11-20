mod adder;
mod flake;
mod remover;
mod verify_getter;

use anyhow::Result;
use rnix::SyntaxNode;

use std::fs;
use std::process::ExitCode;
use std::{env, io, io::prelude::*, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

use clap::{ArgEnum, Parser};

use crate::adder::add_dep;
use crate::remover::remove_dep;
use crate::verify_getter::verify_get;

#[derive(Parser, Debug, Default, Clone)]
#[clap(author, version, about, long_about = None)]
struct Args {
    // dep to add
    #[clap(short, long, value_parser)]
    add: Option<String>,

    // dep to remove
    #[clap(short, long, value_parser)]
    remove: Option<String>,

    // print current deps
    #[clap(short, long, value_parser, default_value = "false")]
    get: bool,

    // filepath for the Nix file
    #[clap(short, long, value_parser)]
    path: Option<String>,

    // human readable output
    #[clap(short, long, value_parser, default_value = "false")]
    human: bool,

    // dep type - regular, python, or flake devShell packages
    #[clap(short, long, arg_enum, default_value = "regular")]
    dep_type: DepType,

    // verbose output
    #[clap(short, long, value_parser, default_value = "false")]
    verbose: bool,

    // Whether or not to write this value directly to the file,
    // or just print it as part of the return message
    #[clap(long, value_parser, default_value = "false")]
    return_output: bool,
}

#[derive(Serialize, Deserialize, Debug)]
enum OpKind {
    #[serde(rename = "add")]
    Add,

    #[serde(rename = "remove")]
    Remove,

    #[serde(rename = "get")]
    Get,
}

#[derive(Serialize, Deserialize, ArgEnum, Clone, Copy, Debug)]
pub enum DepType {
    #[serde(rename = "regular")]
    Regular,

    #[serde(rename = "python")]
    Python,

    #[serde(rename = "flake")]
    Flake,
}

impl Default for DepType {
    fn default() -> Self {
        DepType::Regular
    }
}

#[derive(Serialize, Deserialize)]
struct Op {
    op: OpKind,
    dep_type: Option<DepType>,
    dep: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Res {
    status: String,
    data: Option<String>,
}

type OpResult = std::result::Result<Option<String>, String>;

fn main() -> ExitCode {
    // handle command line args
    let args = Args::parse();
    match real_main(&mut io::stdout(), args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}

fn real_main<W: io::Write>(stdout: &mut W, args: Args) -> Result<(), ()> {
    let replit_nix_file = if matches!(args.dep_type, DepType::Flake) {
        "./flake.nix"
    } else {
        "./replit.nix"
    };
    let default_replit_nix_filepath: String = match env::var("REPL_HOME") {
        Ok(repl_home) => Path::new(repl_home.as_str())
            .join(replit_nix_file)
            .to_str()
            .unwrap()
            .to_string(),
        Err(_) => replit_nix_file.to_string(),
    };

    let flake_nix_filepath = args.path.clone().unwrap_or_else(|| {
        env::var("REPL_HOME")
            .map(|home| Path::new(&home).join("flake.nix").display().to_string())
            .unwrap_or_else(|_| "./flake.nix".to_string())
    });
    let replit_nix_filepath = args.path.unwrap_or_else(|| default_replit_nix_filepath);

    let human_readable = args.human;
    let verbose = args.verbose;

    if args.get {
        if verbose {
            writeln!(stdout, "get_dep").unwrap();
        }

        let result = perform_op(
            stdout,
            OpKind::Get,
            None,
            args.dep_type,
            &replit_nix_filepath,
            verbose,
            args.return_output,
        );
        return report_result(stdout, result, args.dep_type, human_readable);
    }

    // if user explicitly passes in a add or remove dep, then we only handle that specific op
    if let Some(add_dep) = args.add {
        if verbose {
            writeln!(stdout, "add_dep").unwrap();
        }

        let result = perform_op(
            stdout,
            OpKind::Add,
            Some(add_dep),
            args.dep_type,
            &replit_nix_filepath,
            verbose,
            args.return_output,
        );
        return report_result(stdout, result, args.dep_type, human_readable);
    }

    if let Some(remove_dep) = args.remove {
        if verbose {
            writeln!(stdout, "remove_dep").unwrap();
        }

        let result = perform_op(
            stdout,
            OpKind::Remove,
            Some(remove_dep),
            args.dep_type,
            &replit_nix_filepath,
            verbose,
            args.return_output,
        );
        return report_result(stdout, result, args.dep_type, human_readable);
    }

    if verbose {
        writeln!(stdout, "reading from stdin").unwrap();
    }

    let stdin = io::stdin();
    let mut outcome = Ok(());
    for line in stdin.lock().lines() {
        let (result, dep_type) = match line {
            Ok(line) => match from_str::<Op>(&line) {
                Ok(json) => {
                    let dep_type = json.dep_type.unwrap_or(args.dep_type);
                    let result = perform_op(
                        stdout,
                        json.op,
                        json.dep,
                        dep_type,
                        if matches!(dep_type, DepType::Flake) {
                            &flake_nix_filepath
                        } else {
                            &replit_nix_filepath
                        },
                        verbose,
                        args.return_output,
                    );
                    (result, dep_type)
                }
                Err(_) => (Err("Invalid JSON".to_string()), args.dep_type),
            },
            Err(_) => (Err("Could not read stdin".to_string()), args.dep_type),
        };
        if report_result(stdout, result, dep_type, human_readable).is_err() {
            outcome = Err(());
        }
    }
    outcome
}

fn report_result<W: io::Write>(
    stdout: &mut W,
    result: OpResult,
    dep_type: DepType,
    human_readable: bool,
) -> Result<(), ()> {
    // Callers parse per-op JSON only on exit 0, so legacy dep types keep exit 0 on op errors.
    let outcome = if result.is_err() && matches!(dep_type, DepType::Flake) {
        Err(())
    } else {
        Ok(())
    };
    send_res(stdout, result, human_readable);
    outcome
}

const EMPTY_TEMPLATE: &str = r#"{pkgs}: {
  deps = [];
}
"#;

fn perform_op<W: io::Write>(
    stdout: &mut W,
    op: OpKind,
    dep: Option<String>,
    dep_type: DepType,
    replit_nix_filepath: &str,
    verbose: bool,
    return_output: bool,
) -> OpResult {
    if verbose {
        writeln!(stdout, "perform_op: {:?} {:?}", op, dep).unwrap();
    }

    // read replit.nix file
    let contents = match fs::read_to_string(replit_nix_filepath) {
        Ok(contents) => contents,
        // if replit.nix doesn't exist start with an empty one
        Err(err)
            if err.kind() == io::ErrorKind::NotFound && !matches!(dep_type, DepType::Flake) =>
        {
            EMPTY_TEMPLATE.to_string()
        }
        Err(_) => return Err(format!("error: reading file - {:?}", &replit_nix_filepath)),
    };

    if matches!(dep_type, DepType::Flake) && !matches!(op, OpKind::Add) {
        return Err("flake: only add operations are supported".to_string());
    }
    if matches!(dep_type, DepType::Flake) {
        if return_output {
            return edit_flake(&contents, dep).map(Some);
        }
        return write_flake(Path::new(replit_nix_filepath), contents, dep, |_| {});
    }
    let parsed = rnix::Root::parse(&contents);
    let root = parsed.syntax().clone_for_update();

    let deps_list = match verify_get(&root, dep_type) {
        Ok(deps_list) => deps_list,
        Err(_) => return Err("Could not verify and get".to_string()),
    };

    let op_res = match op {
        OpKind::Add => add_dep(deps_list, dep).map(|_| root.to_string()),
        OpKind::Remove => remove_dep(&contents, deps_list.node, dep),
        OpKind::Get => {
            let deps = match get_deps(deps_list.node) {
                Ok(deps) => deps,
                Err(_) => {
                    return Err("Could not get deps".to_string());
                }
            };
            return Ok(Some(deps.join(",")));
        }
    };

    let new_contents = match op_res {
        Ok(new_contents) => new_contents,
        Err(_) => {
            return Err("Could not perform op".to_string());
        }
    };

    if return_output {
        return Ok(Some(new_contents));
    }

    if new_contents == contents {
        return Ok(None);
    }

    // write new replit.nix file
    match atomic_write(Path::new(replit_nix_filepath), &new_contents) {
        Ok(_) => Ok(None),
        Err(err) => Err(format!(
            "Could not write to file {}: {}",
            replit_nix_filepath, err
        )),
    }
}

fn edit_flake(contents: &str, dep: Option<String>) -> Result<String, String> {
    let parsed = rnix::Root::parse(contents);
    if !parsed.errors().is_empty() {
        return Err("flake: invalid Nix syntax".to_string());
    }
    let root = parsed.syntax().clone_for_update();
    let list = verify_get(&root, DepType::Flake).map_err(|err| err.to_string())?;
    flake::add_dep(contents, list, dep).map_err(|_| "Could not perform op".to_string())
}

#[cfg(target_os = "linux")]
fn exchange_files(first: &Path, second: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let first = CString::new(first.as_os_str().as_bytes())?;
    let second = CString::new(second.as_os_str().as_bytes())?;
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            first.as_ptr(),
            libc::AT_FDCWD,
            second.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "linux"))]
fn exchange_files(_first: &Path, _second: &Path) -> io::Result<()> {
    Err(io::Error::from_raw_os_error(libc::ENOSYS))
}

fn write_flake(
    path: &Path,
    contents: String,
    dep: Option<String>,
    mut before_exchange: impl FnMut(usize),
) -> OpResult {
    let write_error =
        |err: io::Error| format!("Could not write to file {}: {}", path.display(), err);
    let destination = if path.is_symlink() {
        fs::canonicalize(path).map_err(write_error)?
    } else {
        path.to_path_buf()
    };
    let dir = destination
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut conflict_name = destination.as_os_str().to_os_string();
    conflict_name.push(".nix-editor-conflict");
    let conflict = std::path::PathBuf::from(conflict_name);
    let preserve = |temporary: tempfile::TempPath| -> Result<(), String> {
        temporary.persist(&conflict).map_err(|err| {
            let _ = err.path.keep();
            write_error(err.error)
        })?;
        fs::File::open(dir)
            .and_then(|dir| dir.sync_all())
            .map_err(write_error)
    };
    let mut expected = contents.clone();
    let mut base = contents;
    let mut displaced_copy = None;
    let result = (|| {
        for attempt in 0..10 {
            let (new, edit_error) = match edit_flake(&base, dep.clone()) {
                Ok(new) => (new, None),
                Err(err) if attempt > 0 => (base.clone(), Some(err)),
                Err(err) => return Err(err),
            };
            if attempt == 0 && new == expected {
                return Ok(None);
            }
            let mut temporary = tempfile::NamedTempFile::new_in(dir).map_err(write_error)?;
            if let Ok(metadata) = fs::metadata(&destination) {
                temporary
                    .as_file()
                    .set_permissions(metadata.permissions())
                    .map_err(write_error)?;
            }
            temporary.write_all(new.as_bytes()).map_err(write_error)?;
            temporary.as_file().sync_all().map_err(write_error)?;
            before_exchange(attempt);
            if let Err(err) = exchange_files(temporary.path(), &destination) {
                if matches!(
                    err.raw_os_error(),
                    Some(libc::EINVAL | libc::ENOSYS | libc::EOPNOTSUPP | libc::ENOENT)
                ) {
                    atomic_write(&destination, &new).map_err(write_error)?;
                    drop(displaced_copy.take());
                    return edit_error.map_or(Ok(None), Err);
                }
                return Err(write_error(err));
            }
            // After exchange the handle refers to the published inode; only the path reads the save.
            let temporary = temporary.into_temp_path();
            let displaced = match fs::read_to_string(&temporary) {
                Ok(displaced) => displaced,
                Err(err) => {
                    drop(displaced_copy.take());
                    preserve(temporary)?;
                    return Err(write_error(err));
                }
            };
            drop(displaced_copy.take());
            if displaced == expected {
                temporary.close().map_err(write_error)?;
                fs::File::open(dir)
                    .and_then(|dir| dir.sync_all())
                    .map_err(write_error)?;
                return edit_error.map_or(Ok(None), Err);
            }
            expected = new;
            base = displaced;
            displaced_copy = Some(temporary);
        }
        preserve(displaced_copy.take().unwrap())?;
        Err(format!(
            "flake: flake.nix kept changing during the edit; previous contents saved to {}",
            conflict.display()
        ))
    })();
    if result.is_err() {
        if let Some(previous) = displaced_copy.take() {
            preserve(previous)?;
        }
    }
    result
}

fn atomic_write(path: &Path, contents: &str) -> io::Result<()> {
    // Preserve the existing behavior of following symbolic links.
    let destination = if path.is_symlink() {
        fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let dir = destination
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(dir)?;
    if let Ok(metadata) = fs::metadata(&destination) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary.write_all(contents.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary.persist(&destination).map_err(|err| err.error)?;
    fs::File::open(dir)?.sync_all()
}

fn send_res<W: io::Write>(stdout: &mut W, result: OpResult, human_readable: bool) {
    let (status, data) = match result {
        Ok(data) => ("success", data),
        Err(message) => ("error", Some(message)),
    };
    if human_readable {
        let mut out = status.to_owned();

        if let Some(data) = data {
            out += &(": ".to_string() + &data);
        }
        writeln!(stdout, "{}", out).unwrap();
        return;
    }

    let res = Res {
        status: status.to_string(),
        data,
    };

    let json = match to_string(&res) {
        Ok(json) => json,
        Err(_) => {
            if human_readable {
                writeln!(stdout, "error: Could not serialize to JSON").unwrap();
            } else {
                let err_msg = r#"{"status": "error", "data": "Could not serialize to JSON"}"#;
                writeln!(stdout, "{}", err_msg).unwrap();
            }
            return;
        }
    };

    writeln!(stdout, "{}", json).unwrap();
}

fn get_deps(deps_list: SyntaxNode) -> Result<Vec<String>> {
    Ok(deps_list
        .children()
        .map(|child| child.text().to_string())
        .collect())
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[cfg(target_os = "linux")]
    const FLAKE: &str = "{ outputs = { pkgs }: { devShells.x86_64-linux.default = pkgs.mkShell { packages = []; }; }; }\n";

    #[cfg(target_os = "linux")]
    fn save_by_rename(path: &Path, contents: &str) {
        let mut save = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
        save.write_all(contents.as_bytes()).unwrap();
        save.persist(path).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn flake_exchange_reconciles_concurrent_save() {
        for already_added in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("flake.nix");
            fs::write(&path, FLAKE).unwrap();
            let saved = format!("{}# saved change\n", FLAKE);
            let saved = if already_added {
                saved.replace("[]", "[ pkgs.jq ]")
            } else {
                saved
            };
            let mut attempts = 0;
            let result = write_flake(
                &path,
                FLAKE.to_string(),
                Some("pkgs.jq".to_string()),
                |attempt| {
                    attempts += 1;
                    if attempt == 0 {
                        save_by_rename(&path, &saved);
                    }
                },
            );
            assert_eq!(result, Ok(None));
            assert_eq!(attempts, 2);
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                saved.replace("[]", "[ pkgs.jq ]")
            );
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn flake_exchange_without_concurrent_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("flake.nix");
        fs::write(&path, FLAKE).unwrap();
        let expected = edit_flake(FLAKE, Some("pkgs.jq".to_string())).unwrap();
        let mut attempts = 0;
        assert_eq!(
            write_flake(
                &path,
                FLAKE.to_string(),
                Some("pkgs.jq".to_string()),
                |_| {
                    attempts += 1;
                }
            ),
            Ok(None)
        );
        assert_eq!(attempts, 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(
            write_flake(&path, expected, Some("pkgs.jq".to_string()), |_| {
                panic!("duplicate must not write");
            }),
            Ok(None)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn flake_exchange_preserves_last_save_on_exhaustion() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("flake.nix");
        fs::write(&path, FLAKE).unwrap();
        let mut attempts = 0;
        let result = write_flake(
            &path,
            FLAKE.to_string(),
            Some("pkgs.jq".to_string()),
            |attempt| {
                attempts += 1;
                save_by_rename(&path, &format!("{}# save {}\n", FLAKE, attempt));
            },
        );
        let conflict = dir.path().join("flake.nix.nix-editor-conflict");
        assert_eq!(attempts, 10);
        assert_eq!(
            result,
            Err(format!(
                "flake: flake.nix kept changing during the edit; previous contents saved to {}",
                conflict.display()
            ))
        );
        assert_eq!(
            fs::read_to_string(conflict).unwrap(),
            format!("{}# save 9\n", FLAKE)
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn flake_exchange_reports_invalid_displaced_base() {
        for (saved, error) in [
            ("{", "flake: invalid Nix syntax"),
            (
                "{}",
                "flake: expected a dotted devShells.x86_64-linux.default binding in outputs",
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("flake.nix");
            fs::write(&path, FLAKE).unwrap();
            let result = write_flake(
                &path,
                FLAKE.to_string(),
                Some("pkgs.jq".to_string()),
                |attempt| {
                    if attempt == 0 {
                        save_by_rename(&path, saved);
                    }
                },
            );
            assert_eq!(result, Err(error.to_string()));
            assert_eq!(fs::read_to_string(&path).unwrap(), saved);
            assert!(!dir.path().join("flake.nix.nix-editor-conflict").exists());
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn flake_exchange_falls_back_when_destination_disappears() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("flake.nix");
        fs::write(&path, FLAKE).unwrap();
        assert_eq!(
            write_flake(
                &path,
                FLAKE.to_string(),
                Some("pkgs.jq".to_string()),
                |_| {
                    fs::remove_file(&path).unwrap();
                }
            ),
            Ok(None)
        );
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            FLAKE.replace("[]", "[ pkgs.jq ]")
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn flake_exchange_follows_symlink_and_preserves_permissions() {
        use std::os::unix::{fs::symlink, fs::PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("target.nix");
        let link = dir.path().join("flake.nix");
        fs::write(&path, FLAKE).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        symlink(&path, &link).unwrap();
        assert_eq!(
            write_flake(
                &link,
                FLAKE.to_string(),
                Some("pkgs.jq".to_string()),
                |_| {}
            ),
            Ok(None)
        );
        assert!(link.is_symlink());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            FLAKE.replace("[]", "[ pkgs.jq ]")
        );
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    const TEMPLATE: &str = r#"{pkgs}: {
  deps = [
    pkgs.cowsay
  ];
}
"#;

    #[test]
    fn test_integration_makes_template_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let repl_nix_file = dir.path().join("replit.nix");
        env::set_var("REPL_HOME", dir.path().display().to_string());

        let args = Args {
            add: Some("pkgs.ncdu".to_string()),
            ..Default::default()
        };
        real_main(&mut io::stdout(), args).unwrap();

        let contents = fs::read_to_string(repl_nix_file.clone()).unwrap();

        assert_eq!(
            r#"{pkgs}: {
  deps = [
    pkgs.ncdu
  ];
}
"#,
            contents
        );

        drop(repl_nix_file);
        dir.close().unwrap();
    }

    #[test]
    fn test_integration_makes_python_ld_library_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let repl_nix_file = dir.path().join("replit.nix");

        fs::write(repl_nix_file.as_os_str(), EMPTY_TEMPLATE.as_bytes()).unwrap();

        let args = Args {
            path: Some(repl_nix_file.clone().display().to_string()),
            dep_type: DepType::Python,
            add: Some("pkgs.zlib".to_string()),
            ..Default::default()
        };
        real_main(&mut io::stdout(), args).unwrap();

        let contents = fs::read_to_string(repl_nix_file.clone()).unwrap();

        assert_eq!(
            r#"{pkgs}: {
  deps = [];
  env = {
    PYTHON_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      pkgs.zlib
    ];
  };
}
"#,
            contents
        );
        drop(repl_nix_file);
        dir.close().unwrap();
    }

    #[test]
    fn test_integration_no_change_no_write() {
        let dir = tempfile::tempdir().unwrap();
        let repl_nix_file = dir.path().join("replit.nix");

        fs::write(repl_nix_file.as_os_str(), EMPTY_TEMPLATE.as_bytes()).unwrap();
        let args = Args {
            path: Some(repl_nix_file.clone().display().to_string()),
            dep_type: DepType::Python,
            add: Some("pkgs.zlib".to_string()),
            ..Default::default()
        };
        real_main(&mut io::stdout(), args.clone()).unwrap();

        let metadata = fs::metadata(repl_nix_file.as_os_str()).unwrap();
        let modification_time = metadata.modified().unwrap();

        real_main(&mut io::stdout(), args).unwrap();

        let metadata = fs::metadata(repl_nix_file.as_os_str()).unwrap();
        let modification_time2 = metadata.modified().unwrap();

        assert_eq!(modification_time, modification_time2);
    }

    #[test]
    fn test_integration_remove_writes() {
        let dir = tempfile::tempdir().unwrap();
        let repl_nix_file = dir.path().join("replit.nix");

        fs::write(repl_nix_file.as_os_str(), TEMPLATE.as_bytes()).unwrap();
        let args = Args {
            path: Some(repl_nix_file.clone().display().to_string()),
            dep_type: DepType::Regular,
            remove: Some("pkgs.cowsay".to_string()),
            ..Default::default()
        };
        real_main(&mut io::stdout(), args.clone()).unwrap();

        let contents = fs::read_to_string(repl_nix_file.clone()).unwrap();

        assert_eq!("{pkgs}: {\n  deps = [\n  ];\n}\n", contents);

        drop(repl_nix_file);
        dir.close().unwrap();
    }

    #[test]
    fn test_integration_get() {
        let dir = tempfile::tempdir().unwrap();
        let repl_nix_file = dir.path().join("replit.nix");

        fs::write(repl_nix_file.as_os_str(), TEMPLATE.as_bytes()).unwrap();
        let args = Args {
            path: Some(repl_nix_file.clone().display().to_string()),
            get: true,
            ..Default::default()
        };
        let mut stdout = Vec::new();
        real_main(&mut stdout, args.clone()).unwrap();

        assert_eq!(
            stdout,
            br#"{"status":"success","data":"pkgs.cowsay"}
"#
        );

        drop(repl_nix_file);
        dir.close().unwrap();
    }
}
