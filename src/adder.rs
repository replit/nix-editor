use anyhow::{Context, Result};
use rnix::SyntaxNode;

pub fn add_dep(
    contents: &mut String,
    deps_list: SyntaxNode,
    new_dep_opt: Option<String>,
) -> Result<String> {
    let new_dep = new_dep_opt.context("error: no dependency")?;

    let dep_list_first_token = deps_list
        .first_token()
        .context("error: could not find first bracket token in deps list")?;
    let open_bracket_pos: usize = dep_list_first_token.text_range().start().into();

    // add dep pos is the character position of the first character of the new dependency
    let add_dep_pos = calc_add_dep_pos(deps_list);

    // we need to add leading whitespace to the next line so that
    // the pkgs are correctly formatted (i.e. they are lined up)
    let white_space_count = if add_dep_pos >= 2 + open_bracket_pos {
        add_dep_pos - open_bracket_pos - 2
    } else {
        2
    };
    let leading_white_space = " ".repeat(white_space_count);

    let new_contents = contents.split_off(add_dep_pos);
    if add_dep_pos == open_bracket_pos + 1 {
        contents.push('\n');
        contents.push_str(&" ".repeat(4));
    }
    contents.push_str(&new_dep);
    contents.push('\n');
    contents.push_str(&leading_white_space);
    contents.push_str(&new_contents);
    Ok(contents.to_string())
}

fn calc_add_dep_pos(deps_list: SyntaxNode) -> usize {
    // get the first child of the deps_list
    // we want to add the new dep right before the first one
    if let Some(first_dep) = deps_list.first_child() {
        first_dep.text_range().start().into()
    } else {
        let deps_list_start: usize = deps_list.text_range().start().into();
        deps_list_start + 1
    }
}

#[cfg(test)]
mod add_tests {
    use super::*;
    use crate::verify_getter::verify_get;
    use crate::DepType;
    use rnix::parse;

    fn test_add(new_dep: &str, initial_contents: &str, expected_contents: &str) {
        let tree = parse(&initial_contents).node();
        let deps_list_res = verify_get(tree, DepType::Regular);
        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap();

        let new_contents = add_dep(&mut initial_contents.to_string(), deps_list, Some(new_dep.to_string()));
        assert!(new_contents.is_ok());

        let new_contents = new_contents.unwrap();

        assert_eq!(new_contents, expected_contents.to_string());
    }

    #[test]
    fn test_empty_regular_add_dep() {
        test_add(
            "pkgs.test",
        r#"
{ pkgs }: {
  deps = [];
}
        "#,
        r#"
{ pkgs }: {
  deps = [
    pkgs.test
  ];
}
        "#)
    }

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
    fn test_regular_add_dep() {
        let mut contents = python_replit_nix();
        let tree = parse(&contents).node();
        let deps_list_res = verify_get(tree, DepType::Regular);
        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap();

        let new_dep = "pkgs.test";

        let new_contents = add_dep(&mut contents, deps_list, Some(new_dep.to_string()));
        assert!(new_contents.is_ok());

        let new_contents = new_contents.unwrap();

        let expected_contents = r#"
{ pkgs }: {
  deps = [
    pkgs.test
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
        .to_string();
        assert_eq!(new_contents, expected_contents);
    }

    #[test]
    fn test_python_add_dep() {
        let mut contents = python_replit_nix();
        let tree = parse(&contents).node();
        let deps_list_res = verify_get(tree, DepType::Python);
        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap();

        let new_dep = "pkgs.test";

        let new_contents = add_dep(&mut contents, deps_list, Some(new_dep.to_string()));
        assert!(new_contents.is_ok());

        let new_contents = new_contents.unwrap();

        let expected_contents = r#"
{ pkgs }: {
  deps = [
    pkgs.python38Full
  ];
  env = {
    PYTHON_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      pkgs.test
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

}
