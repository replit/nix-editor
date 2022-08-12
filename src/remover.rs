use anyhow::{bail, Result};
use rnix::{SyntaxNode, TextRange};

pub fn remove_dep(
    contents: &mut String,
    deps_list: SyntaxNode,
    remove_dep_opt: Option<String>,
) -> Result<String> {
    let remove_dep = match remove_dep_opt {
        Some(remove_dep) => remove_dep,
        None => bail!("error: no dependency to remove"),
    };

    let range_to_remove = match find_remove_dep(deps_list, &remove_dep) {
        Ok(range_to_remove) => range_to_remove,
        Err(_) => bail!("error: could not find dep to remove"),
    };
    let text_start: usize = range_to_remove.start().into();

    // since there may be leading white space, we need to remove the leading white space
    // go backwards char by char until we find non whitespace char
    let remove_start: usize = search_backwards_non_whitespace(text_start, contents);
    let remove_end: usize = range_to_remove.end().into();

    let new_contents = contents.split_off(remove_start);
    let end_section = new_contents
        .chars()
        .skip(remove_end - remove_start)
        .collect::<String>();
    contents.push_str(&end_section);

    Ok(contents.to_string())
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

    let dep = match deps.find(|dep| dep.text() == remove_dep) {
        Some(dep) => dep,
        None => bail!("error: could not find def"),
    };

    Ok(dep.text_range())
}

#[cfg(test)]
mod remove_tests {
    use super::*;
    use crate::verify_getter::verify_get;
    use crate::DepType;
    use rnix::parse;

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
    fn test_regular_remove_dep() {
        let mut contents = python_replit_nix();
        let tree = parse(&contents).node();
        let deps_list_res = verify_get(tree, DepType::Regular);
        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap();

        let dep_to_remove = "pkgs.python38Full";

        let new_contents = remove_dep(&mut contents, deps_list, Some(dep_to_remove.to_string()));
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
        let mut contents = python_replit_nix();
        let tree = parse(&contents).node();
        let deps_list_res = verify_get(tree, DepType::Python);
        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap();

        let dep_to_remove = "pkgs.glib";

        let new_contents = remove_dep(&mut contents, deps_list, Some(dep_to_remove.to_string()));
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
