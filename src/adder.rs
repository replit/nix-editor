use anyhow::{Context, Result};
use rnix::SyntaxNode;

use crate::verify_getter::SyntaxNodeAndWhitespace;

pub fn add_dep(
    deps_list: SyntaxNodeAndWhitespace,
    new_dep_opt: Option<String>,
) -> Result<SyntaxNode> {
    let new_dep = new_dep_opt.context("error: no dependency")?;
    let whitespace = deps_list.whitespace;
    let deps_list = deps_list.node;

    for dep in deps_list.children() {
        if dep.to_string() == new_dep {
            // dep is already present in the deps_list, we're done
            return Ok(deps_list);
        }
    }

    let mut base_indent = 0;
    if let Some(w) = whitespace {
        base_indent = w.text().replace("\n", "").len();
    }
    let entry_indent = base_indent + 2;

    let has_newline = deps_list.to_string().contains('\n');

    let newline = match has_newline {
        true => String::new(),
        false => std::iter::once("\n")
            .chain(std::iter::repeat(" ").take(base_indent))
            .collect(),
    };

    deps_list.splice_children(
        1..1,
        vec![rnix::NodeOrToken::Node(
            rnix::Root::parse(&format!(
                "\n{}{}{newline}",
                &" ".repeat(entry_indent),
                new_dep
            ))
            .syntax()
            .clone_for_update(),
        )],
    );

    Ok(deps_list)
}

#[cfg(test)]
mod add_tests {
    use super::*;
    use crate::verify_getter::verify_get;
    use crate::DepType;

    fn test_add(dep_type: DepType, new_dep: &str, initial_contents: &str, expected_contents: &str) {
        let tree = rnix::Root::parse(&initial_contents)
            .syntax()
            .clone_for_update();

        let deps_list_res = verify_get(&tree, dep_type);
        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap();

        let new_deps_list = add_dep(deps_list, Some(new_dep.to_string()));
        assert!(new_deps_list.is_ok());

        assert_eq!(tree.to_string(), expected_contents.to_string());
    }

    #[test]
    fn test_empty_regular_add_dep() {
        test_add(
            DepType::Regular,
            "pkgs.test",
            r#"{ pkgs }: {
    deps = [];
}
        "#,
            r#"{ pkgs }: {
    deps = [
      pkgs.test
    ];
}
        "#,
        )
    }

    #[test]
    fn test_weird_empty_regular_add_dep() {
        test_add(
            DepType::Regular,
            "pkgs.test",
            r#"{ pkgs }: { deps = []; }"#,
            r#"{ pkgs }: { deps = [
  pkgs.test
]; }"#,
        )
    }

    #[test]
    fn test_empty_but_expanded_regular_add_dep() {
        test_add(
            DepType::Regular,
            "pkgs.test",
            r#"{ pkgs }: {
  deps = [
  ];
}"#,
            r#"{ pkgs }: {
  deps = [
    pkgs.test
  ];
}"#,
        )
    }

    #[test]
    fn test_duplicate_add() {
        test_add(
            DepType::Regular,
            "pkgs.test",
            r#"{ pkgs }: {
  deps = [
    pkgs.test
  ];
}
        "#,
            r#"{ pkgs }: {
  deps = [
    pkgs.test
  ];
}
        "#,
        )
    }

    const PYTHON_REPLIT_NIX: &str = r#"{ pkgs }: {
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
}"#;

    #[test]
    fn test_regular_add_dep() {
        test_add(
            DepType::Regular,
            "pkgs.test",
            PYTHON_REPLIT_NIX,
            r#"{ pkgs }: {
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
}"#,
        );
    }

    #[test]
    fn test_python_add_dep() {
        test_add(
            DepType::Python,
            "pkgs.test",
            PYTHON_REPLIT_NIX,
            r#"{ pkgs }: {
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
}"#,
        );
    }
}
