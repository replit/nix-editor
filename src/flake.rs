use anyhow::{bail, Context, Result};
use rnix::{SyntaxKind::*, SyntaxNode};

use crate::verify_getter::SyntaxNodeAndWhitespace;

const TARGET: &[&str] = &["devShells", "x86_64-linux", "default"];

pub fn add_dep(
    contents: &str,
    list: SyntaxNodeAndWhitespace,
    dep: Option<String>,
) -> Result<String> {
    let dep = dep.context("error: no dependency")?;
    if list
        .node
        .children()
        .any(|existing| existing.to_string() == dep)
    {
        return Ok(contents.to_string());
    }
    let text = list.node.to_string();
    let body = &text[..text.len() - 1];
    let replacement = if !text.contains('\n') {
        format!("{} {} ]", body.trim_end(), dep)
    } else {
        let base_indent = list
            .whitespace
            .as_ref()
            .and_then(|token| token.text().rsplit('\n').next())
            .unwrap_or("");
        let indent = list
            .node
            .children()
            .filter_map(|element| {
                let start = usize::from(element.text_range().start());
                let prefix = contents[..start].rsplit('\n').next()?;
                prefix
                    .chars()
                    .all(|ch| ch == ' ' || ch == '\t')
                    .then(|| prefix.to_string())
            })
            .last()
            .unwrap_or_else(|| format!("{}  ", base_indent));
        match body.rfind('\n') {
            Some(newline) if body[newline + 1..].trim().is_empty() => {
                format!(
                    "{}\n{}{}{}]",
                    &body[..newline],
                    indent,
                    dep,
                    &body[newline..]
                )
            }
            _ => format!("{}\n{}{}\n{}]", body.trim_end(), indent, dep, base_indent),
        }
    };
    let range = list.node.text_range();
    let mut result = contents.to_string();
    result.replace_range(
        usize::from(range.start())..usize::from(range.end()),
        &replacement,
    );
    Ok(result)
}

fn path_is(node: &SyntaxNode, path: &[&str]) -> bool {
    node.kind() == NODE_ATTRPATH_VALUE
        && node.first_child().is_some_and(|key| {
            let parts: Vec<_> = key.children().collect();
            parts.len() == path.len()
                && parts
                    .iter()
                    .zip(path)
                    .all(|(part, name)| part.kind() == NODE_IDENT && part.text() == *name)
        })
}

fn value(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children().nth(1)
}

fn is_default_binding(node: &SyntaxNode) -> bool {
    let mut binding = node.clone();
    let mut path = Vec::new();
    loop {
        if binding.kind() != NODE_ATTRPATH_VALUE {
            return false;
        }
        let Some(key) = binding.first_child() else {
            return false;
        };
        let mut prefix = Vec::new();
        for part in key.children() {
            if part.kind() != NODE_IDENT {
                return false;
            }
            prefix.push(part.to_string());
        }
        prefix.extend(path);
        path = prefix;
        if path == TARGET {
            return true;
        }
        if path.len() >= TARGET.len() {
            return false;
        }
        let Some(parent) = binding
            .parent()
            .filter(|node| node.kind() == NODE_ATTR_SET)
            .and_then(|node| node.parent())
        else {
            return false;
        };
        binding = parent;
    }
}

fn binds_pkgs(node: &SyntaxNode) -> bool {
    node.children().any(|child| {
        path_is(&child, &["pkgs"])
            || (child.kind() == NODE_INHERIT
                && child
                    .children()
                    .any(|name| name.kind() == NODE_IDENT && name.text() == "pkgs"))
    })
}

fn pkgs_in_scope(node: &SyntaxNode) -> bool {
    let mut child = node.clone();
    while let Some(parent) = child.parent() {
        let bound = match parent.kind() {
            NODE_LET_IN => binds_pkgs(&parent),
            NODE_ATTR_SET => {
                parent
                    .children_with_tokens()
                    .any(|part| part.kind() == TOKEN_REC)
                    && binds_pkgs(&parent)
            }
            NODE_LAMBDA if parent.last_child().as_ref() == Some(&child) => {
                parent.first_child().is_some_and(|arg| {
                    (arg.kind() == NODE_IDENT && arg.text() == "pkgs")
                        || (arg.kind() == NODE_PATTERN
                            && arg.children().any(|entry| {
                                entry.first_child().is_some_and(|name| {
                                    name.kind() == NODE_IDENT && name.text() == "pkgs"
                                })
                            }))
                })
            }
            NODE_WITH if parent.last_child().as_ref() == Some(&child) => {
                parent.first_child().is_some_and(|namespace| {
                    namespace.kind() == NODE_IDENT && namespace.text() == "pkgs"
                })
            }
            _ => false,
        };
        if bound {
            return true;
        }
        child = parent;
    }
    false
}

pub fn verify_get(root: &SyntaxNode) -> Result<SyntaxNodeAndWhitespace> {
    let targets: Vec<_> = root.descendants().filter(is_default_binding).collect();
    if targets.len() > 1 {
        bail!("flake: several default shell bindings");
    }
    let target = targets
        .first()
        .context("flake: expected a dotted devShells.x86_64-linux.default binding in outputs")?;
    if target
        .parent()
        .is_some_and(|parent| parent.kind() == NODE_LET_IN)
    {
        bail!("flake: default shell is bound in a let section");
    }
    if !path_is(target, TARGET) {
        bail!("flake: expected a dotted devShells.x86_64-linux.default binding in outputs");
    }

    // Only the attribute set returned by outputs is eligible, not arbitrary nested sets.
    let outputs = root
        .first_child()
        .filter(|node| node.kind() == NODE_ATTR_SET)
        .and_then(|node| node.children().find(|entry| path_is(entry, &["outputs"])))
        .and_then(|node| value(&node));
    let mut output = outputs;
    while output
        .as_ref()
        .is_some_and(|node| matches!(node.kind(), NODE_LAMBDA | NODE_LET_IN | NODE_WITH))
    {
        output = output.and_then(|node| node.last_child());
    }
    if output.as_ref() != target.parent().as_ref() {
        bail!("flake: expected a dotted devShells.x86_64-linux.default binding in outputs");
    }

    let call_error = "flake: default shell must be a direct mkShell or mkShellNoCC call";
    let call = value(target)
        .filter(|node| node.kind() == NODE_APPLY)
        .context(call_error)?;
    let function = call.first_child().context(call_error)?;
    let name = function.to_string();
    if !matches!(
        name.as_str(),
        "pkgs.mkShell" | "pkgs.mkShellNoCC" | "mkShell" | "mkShellNoCC"
    ) {
        bail!(call_error);
    }
    let attrs = value(&call)
        .filter(|node| node.kind() == NODE_ATTR_SET)
        .context(call_error)?;
    let packages: Vec<_> = attrs
        .descendants()
        .filter(|node| {
            path_is(node, &["packages"])
                || (node.kind() == NODE_INHERIT
                    && node
                        .children()
                        .any(|name| name.kind() == NODE_IDENT && name.text() == "packages"))
        })
        .collect();
    if packages.len() != 1
        || packages[0].parent().as_ref() != Some(&attrs)
        || !path_is(&packages[0], &["packages"])
    {
        bail!("flake: shell must have exactly one direct packages binding");
    }
    let list = value(&packages[0])
        .filter(|node| node.kind() == NODE_LIST)
        .context("flake: packages must be a literal list")?;
    if !pkgs_in_scope(&list) {
        bail!("flake: pkgs is not in scope");
    }
    let whitespace = packages[0]
        .prev_sibling_or_token()
        .and_then(|part| part.into_token())
        .filter(|token| token.kind() == TOKEN_WHITESPACE && token.text().contains('\n'))
        .and_then(|token| {
            let indent = token.text().rsplit('\n').next().unwrap_or("");
            rnix::Root::parse(&format!("\n{}", indent))
                .syntax()
                .first_token()
        });
    Ok(SyntaxNodeAndWhitespace {
        whitespace,
        node: list,
    })
}
