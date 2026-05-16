//! btree-cli — a simple command-line interface to btree-rs.
//!
//! Usage:
//!   btree-cli \<db_file\> get \<key\>
//!   btree-cli \<db_file\> set \<key\> \<value\>
//!   btree-cli \<db_file\> del \<key\>
//!   btree-cli \<db_file\> scan [from \<key\>] [to \<key\>]

use btree::{BTree, BTreeError};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: btree-cli <db_file> <command> [args...]");
        eprintln!("Commands: get <key> | set <key> <value> | del <key> | scan");
        std::process::exit(1);
    }

    let db_path = &args[1];
    let command = &args[2];

    let mut tree: BTree<String, String> = BTree::open(db_path, 50).expect("Failed to open db");

    match command.as_str() {
        "get" => {
            if args.len() < 4 {
                eprintln!("Usage: get <key>");
                std::process::exit(1);
            }
            match tree.get(&args[3]) {
                Ok(v) => println!("{}", v),
                Err(BTreeError::KeyNotFound) => {
                    eprintln!("(nil)");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "set" => {
            if args.len() < 5 {
                eprintln!("Usage: set <key> <value>");
                std::process::exit(1);
            }
            tree.insert(args[3].clone(), args[4].clone())
                .expect("Insert failed");
            println!("OK");
        }
        "del" => {
            if args.len() < 4 {
                eprintln!("Usage: del <key>");
                std::process::exit(1);
            }
            match tree.delete(&args[3]) {
                Ok(_) => println!("OK"),
                Err(BTreeError::KeyNotFound) => {
                    eprintln!("(key not found)");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "scan" => {
            let pairs = tree.scan_all().expect("Scan failed");
            if pairs.is_empty() {
                println!("(empty)");
            } else {
                for (k, v) in pairs {
                    println!("{} => {}", k, v);
                }
            }
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            std::process::exit(1);
        }
    }
}
