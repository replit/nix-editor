use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

const TEMPLATE: &str = "{ outputs = { nixpkgs, ... }: let pkgs = nixpkgs.legacyPackages.x86_64-linux; nm = {}; in {\n  devShells.x86_64-linux.default = pkgs.mkShell { packages = [ nm.replit nm.nodejs-24 ]; };\n}; }";

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
            .write_all(b"{\"op\":\"add\",\"dep_type\":\"flake\",\"dep\":\"pkgs.figlet\"}\n")
            .unwrap();
        child.wait_with_output().unwrap()
    } else {
        command
            .args(["--dep-type", "flake", "--add", "pkgs.figlet"])
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
        "[\n  pkgs.figlet\n nm.replit nm.nodejs-24 ]",
    );
    for json in [false, true] {
        let (output, contents) = run(TEMPLATE, json);
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(contents, expected);
    }
}

#[test]
fn empty_list() {
    let source = TEMPLATE.replace("[ nm.replit nm.nodejs-24 ]", "[]");
    let (output, contents) = run(&source, false);
    assert!(output.status.success());
    assert_eq!(contents, source.replace("[]", "[\n  pkgs.figlet\n]"));
}

#[test]
fn multiline_list() {
    let source = TEMPLATE.replace(
        "packages = [ nm.replit nm.nodejs-24 ];",
        "\n    packages = [\n      nm.replit\n      nm.nodejs-24\n    ];",
    );
    let (output, contents) = run(&source, false);
    assert!(output.status.success());
    assert_eq!(
        contents,
        source.replace("packages = [", "packages = [\n      pkgs.figlet")
    );
}

#[test]
fn idempotent_add() {
    let source = TEMPLATE.replace("nm.replit", "pkgs.figlet");
    let (output, contents) = run(&source, false);
    assert!(output.status.success());
    assert_eq!(contents, source);
}

#[test]
fn supported_scope_and_callee_forms() {
    for source in [
        TEMPLATE.replace("pkgs.mkShell", "pkgs.mkShellNoCC"),
        TEMPLATE.replace("pkgs.mkShell", "mkShell"),
        TEMPLATE.replace("pkgs.mkShell", "mkShellNoCC"),
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
            "pkgs.mkShell { packages = [ nm.replit nm.nodejs-24 ]; }",
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
        &TEMPLATE.replace("}; }", "devShells.x86_64-linux.default = other; }; }"),
        "flake: several default shell bindings",
    );
}

#[test]
fn rejects_mixed_default_bindings() {
    refuse(
        &TEMPLATE.replace(
            "}; }",
            "devShells = { x86_64-linux.default = other; }; }; }",
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
    for operation in [vec!["--get"], vec!["--remove", "pkgs.figlet"]] {
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
        .write_all(b"{\"op\":\"add\",\"dep_type\":\"flake\",\"dep\":\"pkgs.figlet\"}\n")
        .unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    assert!(fs::read_to_string(path).unwrap().contains("pkgs.figlet"));
    assert!(!dir.path().join("replit.nix").exists());
}

#[test]
fn duplicate_does_not_replace_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flake.nix");
    let source = TEMPLATE.replace("nm.replit", "pkgs.figlet");
    fs::write(&path, &source).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_nix-editor"))
        .arg("--path")
        .arg(&path)
        .args(["--dep-type", "flake", "--add", "pkgs.figlet"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}
