mod adder;
mod remover;
mod verify_getter;

use anyhow::Result;
use rnix::SyntaxNode;

use std::fs;
use std::{io, io::prelude::*, path::Path, env};

use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

use clap::{ArgEnum, Parser};

use crate::adder::add_dep;
use crate::remover::remove_dep;
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

    // Whether or not to write this value directly to the file,
    // or just print it as part of the return message
    #[clap(long, value_parser, default_value = "false")]
    return_output: bool,
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
    let replit_nix_file = "./replit.nix";
    let default_replit_nix_filepath : String = match env::var("REPL_HOME") {
        Ok(repl_home) => Path::new(repl_home.as_str()).
            join(replit_nix_file).to_str().unwrap().to_string(),
        Err(_) => replit_nix_file.to_string(),
    };

    // handle command line args
    let args = Args::parse();

    let replit_nix_filepath = args
        .path
        .unwrap_or_else(|| default_replit_nix_filepath);

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
            args.return_output,
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
            args.return_output,
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
                    args.return_output,
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
    return_output: bool,
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

    if return_output {
        return ("success".to_string(), Some(new_contents));
    }

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

fn get_deps(deps_list: SyntaxNode) -> Result<Vec<String>> {
    Ok(deps_list
        .children()
        .map(|child| child.text().to_string())
        .collect())
}
