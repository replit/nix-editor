use anyhow::{Context, Result};
use rnix::{SyntaxNode, TextRange};

pub fn remove_dep(
    contents: &str,
    deps_list: SyntaxNode,
    remove_dep_opt: Option<String>,
) -> Result<String> {
    let remove_dep = remove_dep_opt.context("error: expected dep to remove")?;

    let range_to_remove = find_remove_dep(deps_list, &remove_dep)
        .context("error: could not find dependency to remove")?;
    let text_start: usize = range_to_remove.start().into();

    // since there may be leading white space, we need to remove the leading white space
    // go backwards char by char until we find non whitespace char
    let remove_start: usize = search_backwards_non_whitespace(text_start, contents);
    let remove_end: usize = range_to_remove.end().into();

    let (before, _) = contents.split_at(remove_start);
    let after = contents.chars().skip(remove_end).collect::<String>();

    Ok(format!("{}{}", before, after))
}

fn search_backwards_non_whitespace(start_pos: usize, contents: &str) -> usize {
    let mut pos = start_pos;
    while pos > 0 {
        let c = contents.chars().nth(pos - 1).unwrap();
        if !c.is_whitespace() {
            return pos;
        }
        pos -= 1;
    }
    0
}

fn find_remove_dep(deps_list: SyntaxNode, remove_dep: &str) -> Result<TextRange> {
    let mut deps = deps_list.children();

    let dep = deps
        .find(|dep| dep.text() == remove_dep)
        .context("error: could not find dep to remove")?;

    Ok(dep.text_range())
}

#[cfg(test)]
mod remove_tests {
    use super::*;
    use crate::verify_getter::verify_get;
    use crate::DepType;

    fn python_replit_nix() -> String {
        r#"
{ pkgs }: {
  deps = [
    pkgs.python38Full
  ];
  env = {
    PYTHON_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      pkgs.stdenv.cc.cc.lib
      pkgs.zlib
      pkgs.glib
      pkgs.xorg.libX11
    ];
    PYTHONBIN = "${pkgs.python38Full}/bin/python3.8";
    LANG = "en_US.UTF-8";
  };
}
        "#
        .to_string()
    }

    #[test]
    fn test_regular_remove_with_pkgs_dep() {
        let contents = r#"{ pkgs }: {
  deps = with pkgs; [
    pkgs.ncdu
    test
  ];
}
        "#;

        let tree = rnix::Root::parse(&contents).syntax();
        let deps_list_res = verify_get(&tree, DepType::Regular);
        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap();

        let dep_to_remove = "pkgs.ncdu";

        let new_contents = remove_dep(&contents, deps_list.node, Some(dep_to_remove.to_string()));
        assert!(new_contents.is_ok());

        let new_contents = new_contents.unwrap();

        let expected_contents = r#"{ pkgs }: {
  deps = with pkgs; [
    test
  ];
}
        "#;
        assert_eq!(new_contents, expected_contents);
    }

    #[test]
    fn test_regular_remove_dep() {
        let contents = python_replit_nix();
        let tree = rnix::Root::parse(&contents).syntax();
        let deps_list_res = verify_get(&tree, DepType::Regular);
        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap();

        let dep_to_remove = "pkgs.python38Full";

        let new_contents = remove_dep(&contents, deps_list.node, Some(dep_to_remove.to_string()));
        assert!(new_contents.is_ok());

        let new_contents = new_contents.unwrap();

        let expected_contents = r#"
{ pkgs }: {
  deps = [
  ];
  env = {
    PYTHON_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      pkgs.stdenv.cc.cc.lib
      pkgs.zlib
      pkgs.glib
      pkgs.xorg.libX11
    ];
    PYTHONBIN = "${pkgs.python38Full}/bin/python3.8";
    LANG = "en_US.UTF-8";
  };
}
        "#
        .to_string();
        assert_eq!(new_contents, expected_contents);
    }

    #[test]
    fn test_python_remove_dep() {
        let contents = python_replit_nix();
        let tree = rnix::Root::parse(&contents).syntax();
        let deps_list_res = verify_get(&tree, DepType::Python);
        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap();

        let dep_to_remove = "pkgs.glib";

        let new_contents = remove_dep(&contents, deps_list.node, Some(dep_to_remove.to_string()));
        assert!(new_contents.is_ok());

        let new_contents = new_contents.unwrap();

        let expected_contents = r#"
{ pkgs }: {
  deps = [
    pkgs.python38Full
  ];
  env = {
    PYTHON_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      pkgs.stdenv.cc.cc.lib
      pkgs.zlib
      pkgs.xorg.libX11
    ];
    PYTHONBIN = "${pkgs.python38Full}/bin/python3.8";
    LANG = "en_US.UTF-8";
  };
}
        "#
        .to_string();
        assert_eq!(new_contents, expected_contents);
    }
}
