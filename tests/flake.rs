use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

const TEMPLATE: &str = r#"{
  inputs.nixmodules.url = "github:replit/nixmodules";
  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-25.05";
  outputs = { nixmodules, nixpkgs, ... }:
    let
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      nm = nixmodules.packages.x86_64-linux;
    in {
      devShells.x86_64-linux.default = pkgs.mkShellNoCC {
        packages = [ nm.replit nm.nodejs-24 ];
      };
    };
}
"#;

fn run(source: &str, json: bool) -> (std::process::Output, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flake.nix");
    fs::write(&path, source).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_nix-editor"));
    command.arg("--path").arg(&path);
    let output = if json {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"{\"op\":\"add\",\"dep_type\":\"flake\",\"dep\":\"pkgs.jq\"}\n")
            .unwrap();
        child.wait_with_output().unwrap()
    } else {
        command
            .args(["--dep-type", "flake", "--add", "pkgs.jq"])
            .output()
            .unwrap()
    };
    (output, fs::read_to_string(path).unwrap())
}

fn refuse(source: &str, reason: &str) {
    for json in [false, true] {
        let (output, contents) = run(source, json);
        assert!(!output.status.success());
        assert_eq!(contents, source);
        let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["status"], "error");
        assert_eq!(response["data"], reason);
        assert_eq!(String::from_utf8(output.stdout).unwrap().lines().count(), 1);
    }
}

#[test]
fn v2_template_and_json() {
    let expected = TEMPLATE.replace(
        "[ nm.replit nm.nodejs-24 ]",
        "[ nm.replit nm.nodejs-24 pkgs.jq ]",
    );
    for json in [false, true] {
        let (output, contents) = run(TEMPLATE, json);
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(contents, expected);
    }
}

#[test]
fn empty_list() {
    for empty in ["[]", "[ ]"] {
        let source = TEMPLATE.replace("[ nm.replit nm.nodejs-24 ]", empty);
        let (output, contents) = run(&source, false);
        assert!(output.status.success());
        assert_eq!(contents, source.replace(empty, "[ pkgs.jq ]"));
    }
}

#[test]
fn multiline_list() {
    let source = TEMPLATE.replace(
        "packages = [ nm.replit nm.nodejs-24 ];",
        "packages = [\n          nm.replit\n          nm.nodejs-24\n        ];",
    );
    let (output, contents) = run(&source, false);
    assert!(output.status.success());
    assert_eq!(
        contents,
        source.replace("nm.nodejs-24\n", "nm.nodejs-24\n          pkgs.jq\n")
    );
}

#[test]
fn multiline_preserves_element_indentation_and_comments() {
    let source = TEMPLATE.replace(
        "[ nm.replit nm.nodejs-24 ]",
        "[\n\t nm.replit\n\t nm.nodejs-24 # keep this comment\n    ]",
    );
    let (output, contents) = run(&source, false);
    assert!(output.status.success());
    assert_eq!(
        contents,
        source.replace("# keep this comment\n", "# keep this comment\n\t pkgs.jq\n")
    );
}

#[test]
fn empty_multiline_list() {
    let source = TEMPLATE.replace(
        "packages = [ nm.replit nm.nodejs-24 ];",
        "packages = [\n        ];",
    );
    let (output, contents) = run(&source, false);
    assert!(output.status.success());
    assert_eq!(
        contents,
        source.replace("[\n        ]", "[\n          pkgs.jq\n        ]")
    );
}

#[test]
fn idempotent_add() {
    let source = TEMPLATE.replace("nm.replit", "pkgs.jq");
    let (output, contents) = run(&source, false);
    assert!(output.status.success());
    assert_eq!(contents, source);
}

#[test]
fn supported_scope_and_callee_forms() {
    for source in [
        TEMPLATE.replace("pkgs.mkShellNoCC", "pkgs.mkShell"),
        TEMPLATE.replace("pkgs.mkShellNoCC", "mkShell"),
        TEMPLATE.replace("pkgs.mkShellNoCC", "mkShellNoCC"),
        "{ outputs = { pkgs, ... }: { devShells.x86_64-linux.default = pkgs.mkShell { packages = []; }; }; }".to_string(),
        "{ outputs = inputs: with pkgs; { devShells.x86_64-linux.default = mkShell { packages = []; }; }; }".to_string(),
        "{ outputs = inputs: rec { pkgs = inputs.packages; devShells.x86_64-linux.default = pkgs.mkShell { packages = []; }; }; }".to_string(),
    ] {
        assert!(run(&source, false).0.status.success(), "{}", source);
    }
}

#[test]
fn rejects_indirect_call() {
    refuse(
        &TEMPLATE.replace(
            "pkgs.mkShellNoCC {\n        packages = [ nm.replit nm.nodejs-24 ];\n      }",
            "shell",
        ),
        "flake: default shell must be a direct mkShell or mkShellNoCC call",
    );
}

#[test]
fn rejects_wrapped_call() {
    refuse(
        &TEMPLATE.replace("= pkgs.mkShell", "= wrapper pkgs.mkShell"),
        "flake: default shell must be a direct mkShell or mkShellNoCC call",
    );
}

#[test]
fn rejects_nested_attributes() {
    refuse("{ outputs = { pkgs }: { devShells = { x86_64-linux = { default = pkgs.mkShell { packages = []; }; }; }; }; }",
        "flake: expected a dotted devShells.x86_64-linux.default binding in outputs");
}

#[test]
fn rejects_let_default() {
    refuse("{ outputs = { pkgs }: let devShells.x86_64-linux.default = pkgs.mkShell { packages = []; }; in {}; }",
        "flake: default shell is bound in a let section");
}

#[test]
fn rejects_multiple_defaults() {
    refuse(
        &TEMPLATE.replace(
            "    };",
            "      devShells.x86_64-linux.default = other;\n    };",
        ),
        "flake: several default shell bindings",
    );
}

#[test]
fn rejects_mixed_default_bindings() {
    refuse(
        &TEMPLATE.replace(
            "    };",
            "      devShells = { x86_64-linux.default = other; };\n    };",
        ),
        "flake: several default shell bindings",
    );
}

#[test]
fn rejects_missing_packages() {
    refuse(
        &TEMPLATE.replace("packages = [ nm.replit nm.nodejs-24 ];", ""),
        "flake: shell must have exactly one direct packages binding",
    );
}

#[test]
fn rejects_multiple_packages() {
    refuse(
        &TEMPLATE.replace("packages =", "packages = []; packages ="),
        "flake: shell must have exactly one direct packages binding",
    );
}

#[test]
fn rejects_nested_packages() {
    refuse(
        &TEMPLATE.replace("packages =", "nested = { packages = []; }; packages ="),
        "flake: shell must have exactly one direct packages binding",
    );
}

#[test]
fn rejects_concatenation() {
    refuse(
        &TEMPLATE.replace("[ nm.replit nm.nodejs-24 ]", "[] ++ []"),
        "flake: packages must be a literal list",
    );
}

#[test]
fn rejects_variable_list() {
    refuse(
        &TEMPLATE.replace("[ nm.replit nm.nodejs-24 ]", "dependencies"),
        "flake: packages must be a literal list",
    );
}

#[test]
fn rejects_with_list() {
    refuse(
        &TEMPLATE.replace("[ nm.replit nm.nodejs-24 ]", "with pkgs; []"),
        "flake: packages must be a literal list",
    );
}

#[test]
fn rejects_only_build_inputs() {
    refuse(
        &TEMPLATE.replace("packages =", "buildInputs ="),
        "flake: shell must have exactly one direct packages binding",
    );
}

#[test]
fn rejects_only_native_build_inputs() {
    refuse(
        &TEMPLATE.replace("packages =", "nativeBuildInputs ="),
        "flake: shell must have exactly one direct packages binding",
    );
}

#[test]
fn rejects_missing_pkgs_scope() {
    refuse(
        &TEMPLATE.replace("pkgs = nixpkgs.legacyPackages.x86_64-linux;", ""),
        "flake: pkgs is not in scope",
    );
}

#[test]
fn rejects_nonrecursive_sibling_scope() {
    refuse("{ outputs = inputs: { pkgs = inputs.packages; devShells.x86_64-linux.default = pkgs.mkShell { packages = []; }; }; }",
        "flake: pkgs is not in scope");
}

#[test]
fn rejects_unrelated_default() {
    refuse("{ outputs = { pkgs }: { unrelated = { devShells.x86_64-linux.default = pkgs.mkShell { packages = []; }; }; }; }",
        "flake: expected a dotted devShells.x86_64-linux.default binding in outputs");
}

#[test]
fn rejects_invalid_syntax() {
    refuse(&format!("{} ;", TEMPLATE), "flake: invalid Nix syntax");
}

#[test]
fn rejects_inherited_packages_alongside_list() {
    refuse(
        &TEMPLATE.replace("packages =", "inherit packages; packages ="),
        "flake: shell must have exactly one direct packages binding",
    );
}

#[test]
fn refuses_other_operations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flake.nix");
    fs::write(&path, TEMPLATE).unwrap();
    for operation in [vec!["--get"], vec!["--remove", "pkgs.jq"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_nix-editor"))
            .arg("--path")
            .arg(&path)
            .args(["--dep-type", "flake"])
            .args(operation)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["data"], "flake: only add operations are supported");
        assert_eq!(fs::read_to_string(&path).unwrap(), TEMPLATE);
    }
}

#[test]
fn stdin_flake_defaults_to_flake_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flake.nix");
    fs::write(&path, TEMPLATE).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_nix-editor"))
        .current_dir(dir.path())
        .env_remove("REPL_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"op\":\"add\",\"dep_type\":\"flake\",\"dep\":\"pkgs.jq\"}\n")
        .unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        TEMPLATE.replace("nm.nodejs-24 ]", "nm.nodejs-24 pkgs.jq ]")
    );
    assert!(!dir.path().join("replit.nix").exists());
}

#[test]
fn duplicate_does_not_replace_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flake.nix");
    let source = TEMPLATE.replace("nm.replit", "pkgs.jq");
    fs::write(&path, &source).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_nix-editor"))
        .arg("--path")
        .arg(&path)
        .args(["--dep-type", "flake", "--add", "pkgs.jq"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
fn cli_errors_keep_legacy_exit_codes_and_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.nix");
    fs::write(&path, TEMPLATE).unwrap();
    for dep_type in ["regular", "python", "flake"] {
        let output = Command::new(env!("CARGO_BIN_EXE_nix-editor"))
            .arg("--path")
            .arg(&path)
            .args(["--dep-type", dep_type, "--get"])
            .output()
            .unwrap();
        if dep_type == "flake" {
            assert_eq!(output.status.code(), Some(1));
            assert_eq!(
                output.stdout,
                b"{\"status\":\"error\",\"data\":\"flake: only add operations are supported\"}\n"
            );
        } else {
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(
                output.stdout,
                b"{\"status\":\"error\",\"data\":\"Could not verify and get\"}\n"
            );
        }
    }
    fs::write(&path, "{ pkgs }: { deps = [ pkgs.jq ]; }").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_nix-editor"))
        .arg("--path")
        .arg(&path)
        .arg("--get")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        b"{\"status\":\"success\",\"data\":\"pkgs.jq\"}\n"
    );
}

#[test]
fn stdin_reports_every_op_and_defers_flake_failure_until_eof() {
    for flake_failure in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("flake.nix"), TEMPLATE).unwrap();
        fs::write(dir.path().join("replit.nix"), "{ pkgs }: { deps = []; }").unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_nix-editor"))
            .current_dir(dir.path())
            .env_remove("REPL_HOME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut ops = vec![
            (
                "{\"op\":\"add\",\"dep_type\":\"regular\"}",
                "{\"status\":\"error\",\"data\":\"Could not perform op\"}\n",
            ),
            (
                "{\"op\":\"add\",\"dep_type\":\"python\"}",
                "{\"status\":\"error\",\"data\":\"Could not perform op\"}\n",
            ),
        ];
        if flake_failure {
            ops.push((
                "{\"op\":\"get\",\"dep_type\":\"flake\"}",
                "{\"status\":\"error\",\"data\":\"flake: only add operations are supported\"}\n",
            ));
        }
        ops.extend([
            (
                "{\"op\":\"add\",\"dep_type\":\"flake\",\"dep\":\"pkgs.jq\"}",
                "{\"status\":\"success\",\"data\":null}\n",
            ),
            (
                "{\"op\":\"get\",\"dep_type\":\"regular\"}",
                "{\"status\":\"success\",\"data\":\"\"}\n",
            ),
        ]);
        for (op, expected) in ops {
            writeln!(input, "{}", op).unwrap();
            input.flush().unwrap();
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            assert_eq!(line, expected);
            assert!(child.try_wait().unwrap().is_none());
        }
        drop(input);
        assert_eq!(child.wait().unwrap().code(), Some(i32::from(flake_failure)));
        assert_eq!(
            fs::read_to_string(dir.path().join("flake.nix")).unwrap(),
            TEMPLATE.replace("nm.nodejs-24 ]", "nm.nodejs-24 pkgs.jq ]")
        );
    }
}
