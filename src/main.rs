mod verify_getter;

use rnix::*;
use std::fs;
use std::{io, io::prelude::*};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

use clap::{ArgEnum, Parser};

use crate::verify_getter::verify_get;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    // dep to add
    #[clap(short, long, value_parser)]
    add: Option<String>,

    // dep to remove
    #[clap(short, long, value_parser)]
    remove: Option<String>,

    // filepath for replit.nix file
    #[clap(short, long, value_parser)]
    path: Option<String>,

    // human readable output
    #[clap(short, long, value_parser, default_value = "false")]
    human: bool,

    // dep type - used for setting special dep types in the replit.nix file
    #[clap(short, long, arg_enum, default_value = "regular")]
    dep_type: DepType,

    // verbose output
    #[clap(short, long, value_parser, default_value = "false")]
    verbose: bool,
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

fn main() {
    let default_replit_nix_filepath = "replit.nix";

    // handle command line args
    let args = Args::parse();

    let replit_nix_filepath = args
        .path
        .unwrap_or_else(|| default_replit_nix_filepath.to_string());

    let human_readable = args.human;
    let verbose = args.verbose;

    // if user explicitly passes in a add or remove dep, then we only handle that specific op
    if let Some(add_dep) = args.add {
        if verbose {
            println!("add_dep");
        }

        let (status, data) = perform_op(
            OpKind::Add,
            Some(add_dep),
            args.dep_type,
            &replit_nix_filepath,
            verbose,
        );
        send_res(&status, data, human_readable);
        return;
    }

    if let Some(remove_dep) = args.remove {
        if verbose {
            println!("remove_dep");
        }

        let (status, data) = perform_op(
            OpKind::Remove,
            Some(remove_dep),
            args.dep_type,
            &replit_nix_filepath,
            verbose,
        );
        send_res(&status, data, human_readable);
        return;
    }

    if verbose {
        println!("reading from stdin");
    }

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        match line {
            Ok(line) => {
                let json: Op = match from_str(&line) {
                    Ok(json_val) => json_val,
                    Err(_) => {
                        send_res("error", Some("Invalid JSON".to_string()), human_readable);
                        continue;
                    }
                };

                let (status, data) = perform_op(
                    json.op,
                    json.dep,
                    json.dep_type.unwrap_or(args.dep_type),
                    &replit_nix_filepath,
                    verbose,
                );
                send_res(&status, data, human_readable);
            }
            Err(_) => {
                send_res(
                    "error",
                    Some("Could not read stdin".to_string()),
                    human_readable,
                );
            }
        }
    }
}

fn perform_op(
    op: OpKind,
    dep: Option<String>,
    dep_type: DepType,
    replit_nix_filepath: &str,
    verbose: bool,
) -> (String, Option<String>) {
    if verbose {
        println!("perform_op: {:?} {:?}", op, dep);
    }

    // read replit.nix file
    let mut contents = match fs::read_to_string(replit_nix_filepath) {
        Ok(contents) => contents,
        Err(_) => {
            return (
                "error".to_string(),
                Some(format!("Could not read file {}", replit_nix_filepath)),
            );
        }
    };

    let ast = rnix::parse(&contents);

    let deps_list = match verify_get(ast.node(), dep_type) {
        Ok(deps_list) => deps_list,
        Err(_) => {
            return (
                "error".to_string(),
                Some("Could not verify and get".to_string()),
            );
        }
    };

    let op_res = match op {
        OpKind::Add => add_dep(&mut contents, deps_list, dep),
        OpKind::Remove => remove_dep(&mut contents, deps_list, dep),
        OpKind::Get => {
            let deps = match get_deps(deps_list) {
                Ok(deps) => deps,
                Err(_) => {
                    return ("error".to_string(), Some("Could not get deps".to_string()));
                }
            };
            return ("success".to_string(), Some(deps.join(",")));
        }
    };

    let new_contents = match op_res {
        Ok(new_contents) => new_contents,
        Err(_) => {
            return (
                "error".to_string(),
                Some("Could not perform op".to_string()),
            );
        }
    };

    // write new replit.nix file
    match fs::write(&replit_nix_filepath, new_contents) {
        Ok(_) => ("success".to_string(), None),
        Err(_) => (
            "error".to_string(),
            Some(format!("Could not write to file {}", replit_nix_filepath)),
        ),
    }
}

fn send_res(status: &str, data: Option<String>, human_readable: bool) {
    if human_readable {
        let mut out = status.to_owned();

        if let Some(data) = data {
            out += &(": ".to_string() + &data);
        }
        println!("{}", out);
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
                println!("error: Could not serialize to JSON");
            } else {
                let err_msg = r#"{"status": "error", "data": "Could not serialize to JSON"}"#;
                println!("{}", err_msg);
            }
            return;
        }
    };

    println!("{}", json);
}

fn add_dep(
    contents: &mut String,
    deps_list: SyntaxNode,
    new_dep_opt: Option<String>,
) -> Result<String> {
    let new_dep = match new_dep_opt {
        Some(new_dep) => new_dep,
        None => bail!("error: no new dependency"),
    };

    let open_bracket_pos: usize = match deps_list.first_token() {
        Some(token) => token.text_range().start().into(),
        None => bail!("error: could not find first bracket token in deps list"),
    };

    // add dep pos is the character position of the first character of the new dependency
    let add_dep_pos = calc_add_dep_pos(deps_list);

    // we need to add leading whitespace to the next line so that
    // the pkgs are correctly formatted (i.e. they are lined up)
    let white_space_count = if add_dep_pos >= 2 + open_bracket_pos {
        add_dep_pos - open_bracket_pos - 2
    } else {
        0
    };
    let leading_white_space = " ".repeat(white_space_count);

    let new_contents = contents.split_off(add_dep_pos);
    contents.push_str(&new_dep);
    contents.push('\n');
    contents.push_str(&leading_white_space);
    contents.push_str(&new_contents);
    Ok(contents.to_string())
}

fn remove_dep(
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

fn get_deps(deps_list: SyntaxNode) -> Result<Vec<String>> {
    Ok(deps_list
        .children()
        .map(|child| child.text().to_string())
        .collect())
}

fn find_remove_dep(deps_list: SyntaxNode, remove_dep: &str) -> Result<TextRange> {
    let mut deps = deps_list.children();

    let dep = match deps.find(|dep| dep.text() == remove_dep) {
        Some(dep) => dep,
        None => bail!("error: could not find def"),
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
