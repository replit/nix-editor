use anyhow::{bail, Context, Result};
use rnix::*;

use crate::DepType;

// kind of like assert! but returns an error instead of panicking
macro_rules! verify_eq {
    ($a:expr, $b:expr) => {
        if $a != $b {
            bail!(
                "error: expected {} but got {}",
                stringify!($b),
                stringify!($a)
            );
        }
    };
}

pub struct SyntaxNodeAndWhitespace {
    pub whitespace: Option<SyntaxToken>,
    pub node: SyntaxNode
}

// Will try to parse through the AST and return a list of deps
// If at any point, the tree is not *exactly* how we expect it to look,
// it will return an error. Since nix is so complex, we have to require some
// assumptions about the AST, or else it'll be impossible to do anything.
pub fn verify_get(root: &SyntaxNode, dep_type: DepType) -> Result<SyntaxNodeAndWhitespace> {
    verify_eq!(root.kind(), SyntaxKind::NODE_ROOT);

    let lambda = get_nth_child(&root, 0).context("expected to have a child")?;
    verify_eq!(lambda.kind(), SyntaxKind::NODE_LAMBDA);

    let arg_pattern = get_nth_child(&lambda, 0).context("expected to have a child")?;
    verify_eq!(arg_pattern.kind(), SyntaxKind::NODE_PATTERN);

    if find_child_with_value(&arg_pattern, "pkgs").is_none() {
        bail!("error: expected pkgs");
    }

    let attr_set = get_nth_child(&lambda, 1).context("expected to have two children")?;
    verify_eq!(attr_set.kind(), SyntaxKind::NODE_ATTR_SET);

    let deps_list = match dep_type {
        DepType::Regular => verify_get_regular(&attr_set)?,
        DepType::Python => verify_get_python(&attr_set)?,
    };

    Ok(deps_list)
}

fn verify_get_regular(attr_set: &SyntaxNode) -> Result<SyntaxNodeAndWhitespace> {
    let deps = find_key_value_with_key(&attr_set, "deps").context("expected to have a deps key")?;
    let whitespace = deps.whitespace;
    let deps = deps.node;
    verify_eq!(deps.kind(), SyntaxKind::NODE_ATTRPATH_VALUE);

    let deps_list = get_nth_child(&deps, 1).context("expected to have two children")?;
    verify_eq!(deps_list.kind(), SyntaxKind::NODE_LIST);

    Ok(SyntaxNodeAndWhitespace { whitespace, node: deps_list})
}

fn verify_get_python(attr_set: &SyntaxNode) -> Result<SyntaxNodeAndWhitespace> {
    let env = find_key_value_with_key(&attr_set, "env").context("expected to have an env key")?.node;
    verify_eq!(env.kind(), SyntaxKind::NODE_ATTRPATH_VALUE);

    let env_attr_set = get_nth_child(&env, 1).context("expected to have two children")?;
    verify_eq!(env_attr_set.kind(), SyntaxKind::NODE_ATTR_SET);

    let py_lib_path = find_key_value_with_key(&env_attr_set, "PYTHON_LD_LIBRARY_PATH")
        .context("expected to have a PYTHON_LD_LIBRARY_PATH key")?;
    let whitespace = py_lib_path.whitespace;
    let py_lib_path = py_lib_path.node;
    verify_eq!(py_lib_path.kind(), SyntaxKind::NODE_ATTRPATH_VALUE);

    let py_lib_apply = get_nth_child(&py_lib_path, 1).context("expected to have two children")?;
    verify_eq!(py_lib_apply.kind(), SyntaxKind::NODE_APPLY);

    let py_lib_node_select = get_nth_child(&py_lib_apply, 0).context("expected to have a child")?;
    verify_eq!(py_lib_node_select.kind(), SyntaxKind::NODE_SELECT);
    verify_eq!(py_lib_node_select.text(), "pkgs.lib.makeLibraryPath");

    let py_lib_node_list =
        get_nth_child(&py_lib_apply, 1).context("expected to have two children")?;
    verify_eq!(py_lib_node_list.kind(), SyntaxKind::NODE_LIST);

    Ok(SyntaxNodeAndWhitespace { whitespace, node: py_lib_node_list})
}

fn get_nth_child(node: &SyntaxNode, index: usize) -> Option<SyntaxNode> {
    node.children().into_iter().nth(index)
}

fn find_child_with_value(node: &SyntaxNode, name: &str) -> Option<SyntaxNode> {
    node.children()
        .into_iter()
        .find(|child| child.text() == name)
}

fn find_key_value_with_key(node: &SyntaxNode, key: &str) -> Option<SyntaxNodeAndWhitespace> {
    if node.kind() != SyntaxKind::NODE_ATTR_SET {
        return None;
    }

    let mut last_whitespace = None;

    let node = node.children_with_tokens().into_iter().find(|child| {
        if let Some(token) = child.as_token() {
            if token.kind() != SyntaxKind::TOKEN_WHITESPACE {
                return false;
            }
            let w = token.text();
            if !w.contains("\n") {
                return false;
            }
            last_whitespace = Some(token.clone());
            return false;
        }
        if child.as_node().is_none() {
            return false;
        }

        let node = child.as_node().unwrap();

        if node.kind() != SyntaxKind::NODE_ATTRPATH_VALUE {
            return false;
        }

        let key_node = match get_nth_child(node, 0) {
            Some(child) => child,
            None => return false,
        };

        key_node.text() == key
    });

    match node {
        Some(node_or_token) =>
             Some(SyntaxNodeAndWhitespace {
                 whitespace: last_whitespace,
                 node: node_or_token.as_node().unwrap().clone(),
             }),
        _ => None,
    }
}

// unit tests
#[cfg(test)]
mod verify_get_tests {
    use super::*;

    fn python_replit_nix_ast() -> SyntaxNode {
        let code = r#"
{ pkgs }: {
  deps = [
    pkgs.python38Full
  ];
  env = {
    PYTHON_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      # Needed for pandas / numpy
      pkgs.stdenv.cc.cc.lib
      pkgs.zlib
      # Needed for pygame
      pkgs.glib
      # Needed for matplotlib
      pkgs.xorg.libX11
    ];
    PYTHONBIN = "${pkgs.python38Full}/bin/python3.8";
    LANG = "en_US.UTF-8";
  };
}
        "#;
        rnix::Root::parse(code).syntax()
    }

    #[test]
    fn verify_get_python() {
        let ast = python_replit_nix_ast();
        let deps_list_res = verify_get(&ast, DepType::Python);

        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap();
        let whitespace = deps_list.whitespace.unwrap();
        assert_eq!(whitespace.to_string().len(), 5);

        let deps_list = deps_list.node;
        let deps_list_children: Vec<SyntaxNode> = deps_list.children().collect();

        assert_eq!(deps_list_children.len(), 4);

        let deps_list_children_names = deps_list_children
            .iter()
            .map(|child| child.text())
            .collect::<Vec<_>>();
        assert_eq!(
            deps_list_children_names,
            vec![
                "pkgs.stdenv.cc.cc.lib",
                "pkgs.zlib",
                "pkgs.glib",
                "pkgs.xorg.libX11"
            ]
        );

        for child in deps_list_children {
            assert_eq!(child.kind(), SyntaxKind::NODE_SELECT);
        }
    }

    #[test]
    fn verify_get_regular() {
        let ast = python_replit_nix_ast();
        let deps_list_res = verify_get(&ast, DepType::Regular);

        assert!(deps_list_res.is_ok());

        let deps_list = deps_list_res.unwrap().node;
        let deps_list_children: Vec<SyntaxNode> = deps_list.children().collect();

        assert_eq!(deps_list_children.len(), 1);
        assert_eq!(deps_list_children[0].text(), "pkgs.python38Full");
        assert_eq!(deps_list_children[0].kind(), SyntaxKind::NODE_SELECT);
    }
}
