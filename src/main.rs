use rnix::*;
use std::{env, fs};
use std::{io, io::prelude::*, io::Error, io::ErrorKind};

use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

#[derive(Serialize, Deserialize)]
struct Op {
    op: String,
    dep: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Res {
    status: String,
    data: Option<String>,
}

fn main() {
    let default_replit_nix_filepath = "replit.nix";
    let mut args = env::args();

    let arg1 = args
        .nth(1)
        .unwrap_or_else(|| default_replit_nix_filepath.to_string());

    if arg1 == "--info" {
        println!("Version: {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let replit_nix_filepath = arg1;

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        match line {
            Ok(line) => {
                let json: Op = match from_str(&line) {
                    Ok(json_val) => json_val,
                    Err(_) => {
                        send_res("error", Some("Invalid JSON".to_string()));
                        continue;
                    }
                };

                // read replit.nix file
                let mut contents = match fs::read_to_string(&replit_nix_filepath) {
                    Ok(contents) => contents,
                    Err(_) => {
                        send_res(
                            "error",
                            Some(
                                format!("Could not read file {}", &replit_nix_filepath).to_string(),
                            ),
                        );
                        continue;
                    }
                };

                let ast = rnix::parse(&contents);

                let deps_list = match verify_get(ast.node()) {
                    Ok(deps_list) => deps_list,
                    Err(_) => {
                        send_res("error", Some("Could not verify and get".to_string()));
                        continue;
                    }
                };

                let op_res = match json.op.as_str() {
                    "add" => add_dep(&mut contents, deps_list, json.dep),
                    "remove" => remove_dep(&mut contents, deps_list, json.dep),
                    unknown_op => {
                        send_res(
                            "error",
                            Some(format!("Unknown operation {}", unknown_op).to_string()),
                        );
                        continue;
                    }
                };

                let new_contents = match op_res {
                    Ok(new_contents) => new_contents,
                    Err(_) => {
                        send_res("error", Some("Could not perform op".to_string()));
                        continue;
                    }
                };

                // write new replit.nix file
                match fs::write(&replit_nix_filepath, new_contents) {
                    Ok(_) => {
                        send_res("success", None);
                    }
                    Err(_) => {
                        send_res(
                            "error",
                            Some(
                                format!("Could not write to file {}", replit_nix_filepath)
                                    .to_string(),
                            ),
                        );
                    }
                };
            }
            Err(_) => {
                send_res("error", Some("Could not read stdin".to_string()));
            }
        }
    }
}

fn send_res(status: &str, data: Option<String>) {
    let res = Res {
        status: status.to_string(),
        data,
    };

    let json = match to_string(&res) {
        Ok(json) => json,
        Err(_) => {
            println!("error: could not serialize res");
            return;
        }
    };

    println!("{}", json);
}

fn add_dep(
    contents: &mut String,
    deps_list: SyntaxNode,
    new_dep_opt: Option<String>,
) -> Result<String, Error> {
    let new_dep = match new_dep_opt {
        Some(new_dep) => new_dep,
        None => {
            return Err(Error::new(ErrorKind::Other, "no new dependency"));
        }
    };

    // add dep pos is the character position of the first character of the new dependency
    let add_dep_pos = calc_add_dep_pos(deps_list);
    let new_contents = contents.split_off(add_dep_pos);
    contents.push_str(&new_dep);
    contents.push('\n');
    contents.push_str(&new_contents);
    Ok(contents.to_string())
}

fn remove_dep(
    contents: &mut String,
    deps_list: SyntaxNode,
    remove_dep_opt: Option<String>,
) -> Result<String, Error> {
    let remove_dep = match remove_dep_opt {
        Some(remove_dep) => remove_dep,
        None => {
            return Err(Error::new(
                ErrorKind::Other,
                "error: no dependency to remove",
            ));
        }
    };

    let range_to_remove = match find_remove_dep(deps_list, &remove_dep) {
        Ok(range_to_remove) => range_to_remove,
        Err(_) => {
            return Err(Error::new(
                ErrorKind::Other,
                "error: could not find dependency to remove",
            ));
        }
    };
    let remove_start: usize = range_to_remove.start().into();
    let remove_end: usize = range_to_remove.end().into();

    let new_contents = contents.split_off(remove_start);
    let end_section = new_contents
        .chars()
        .skip(remove_end - remove_start)
        .collect::<String>();
    contents.push_str(&end_section);

    Ok(contents.to_string())
}

fn find_remove_dep(deps_list: SyntaxNode, remove_dep: &str) -> Result<TextRange, Error> {
    let mut deps = deps_list.children();

    let dep = match deps.find(|dep| dep.text() == remove_dep) {
        Some(dep) => dep,
        None => {
            return Err(Error::new(ErrorKind::Other, "Could not find dependency"));
        }
    };

    Ok(dep.text_range())
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

fn get_nth_child(node: &SyntaxNode, index: usize) -> Option<SyntaxNode> {
    node.children().into_iter().nth(index)
}

fn find_child_with_value(node: &SyntaxNode, name: &str) -> Option<SyntaxNode> {
    node.children()
        .into_iter()
        .find(|child| child.text() == name)
}

fn find_key_value_with_key(node: &SyntaxNode, key: &str) -> Option<SyntaxNode> {
    if node.kind() != SyntaxKind::NODE_ATTR_SET {
        return None;
    }

    node.children().into_iter().find(|child| {
        if child.kind() != SyntaxKind::NODE_KEY_VALUE {
            return false;
        }

        let key_node = match get_nth_child(child, 0) {
            Some(child) => child,
            None => return false,
        };

        key_node.text() == key
    })
}

fn verify_get(root: SyntaxNode) -> Result<SyntaxNode, Error> {
    // kind of like assert! but returns an error instead of panicking
    macro_rules! verify_eq {
        ($a:expr, $b:expr) => {
            if $a != $b {
                return Err(Error::new(ErrorKind::Other, "Expected equal"));
            }
        };
    }

    macro_rules! unwrap_or_return {
        ($e:expr) => {
            match $e {
                Some(e) => e,
                None => return Err(Error::new(ErrorKind::Other, "Expected Some")),
            }
        };
    }

    verify_eq!(root.kind(), SyntaxKind::NODE_ROOT);

    let lambda = unwrap_or_return!(get_nth_child(&root, 0));
    verify_eq!(lambda.kind(), SyntaxKind::NODE_LAMBDA);

    let arg_pattern = unwrap_or_return!(get_nth_child(&lambda, 0));
    verify_eq!(arg_pattern.kind(), SyntaxKind::NODE_PATTERN);

    if find_child_with_value(&arg_pattern, "pkgs").is_none() {
        return Err(Error::new(ErrorKind::Other, "Expected pkgs"));
    }

    let attr_set = unwrap_or_return!(get_nth_child(&lambda, 1));
    verify_eq!(attr_set.kind(), SyntaxKind::NODE_ATTR_SET);

    let deps = unwrap_or_return!(find_key_value_with_key(&attr_set, "deps"));
    verify_eq!(deps.kind(), SyntaxKind::NODE_KEY_VALUE);

    let deps_list = unwrap_or_return!(get_nth_child(&deps, 1));
    verify_eq!(deps_list.kind(), SyntaxKind::NODE_LIST);

    Ok(deps_list)
}
