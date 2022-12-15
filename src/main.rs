use std::{
    fs,
    io::{self, prelude::*, BufRead, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use clap::Parser;
use lazy_static::lazy_static;
use regex::Regex;
use yaml_rust::YamlLoader;

/// A tool to clean up V25 log files
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// directory to clean
    #[arg(short, long)]
    dirname: String,

    /// check files regardless if cleaned before
    #[arg(short, long, default_value_t = false)]
    force: bool,

    /// verbose print output
    #[arg(long, default_value_t = false)]
    verbose: bool,
}

// TODO: put functions in lib file

/// load_yml loads a yaml file, used here to specifiy minimum number of lines per file type.
fn load_yml(filename: &PathBuf) -> Vec<yaml_rust::Yaml> {
    let mut file =
        fs::File::open(filename).unwrap_or_else(|_| panic!("could not open: {:?}", filename));
    let mut content = String::new();
    file.read_to_string(&mut content)
        .unwrap_or_else(|_| panic!("could not read: {:?}", filename));
    YamlLoader::load_from_str(&content)
        .unwrap_or_else(|_| panic!("could not read to yaml: {:?}", filename))
}

/// lines_from_file reades all lines from a text file and returns them
/// as a vector of strings.
fn lines_from_file(filename: impl AsRef<Path>) -> Result<Vec<String>, io::Error> {
    // this could return a file not found error:
    let file = fs::File::open(filename)?;
    let buf = io::BufReader::new(file);
    // can return an error if file is not readable:
    buf.lines().collect::<Result<Vec<String>, io::Error>>()
}

/// lines_to_file writes a vector of strings to a textfile. trims lines before write.
fn lines_to_file(filename: impl AsRef<Path>, content: Vec<String>) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true) // fully truncate existing content
        .open(filename)?;
    for line in content.iter() {
        writeln!(file, "{}", line)?;
    }
    Ok(())
}

/// write_OSC is a special write function that updates OSC files by prefixing datetime to each line of data
fn write_osc(
    filename: impl AsRef<Path>,
    content: Vec<String>,
    nl_head: usize,
    data_prefix: &str,
) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true) // fully truncate existing content
        .open(filename)?;
    // write header
    for line in content[0..nl_head].iter() {
        writeln!(file, "{}", line)?;
    }
    // write data
    for line in content[nl_head..content.len() - 1].iter() {
        writeln!(file, "\t{}{}", data_prefix, line)?;
    }
    Ok(())
}

/// n_data_fields takes a string, trims surrounding whitespaces and splits jit on delimiter.
/// returns number of fields returned from split.
fn n_data_fields(s: &String, delimiter: &str) -> usize {
    s.trim().split(delimiter).collect::<Vec<&str>>().len()
}

/// n_chars_last_field returns the number of characters found in the last field of a
/// delimited string.
fn n_chars_last_field(s: &String, delimiter: &str) -> Option<usize> {
    match s.trim().split(delimiter).collect::<Vec<&str>>().last() {
        Some(field) => Some(field.chars().count()),
        None => None,
    }
}

/// get_cfg_path returns the directory where the cfg file is expected
fn get_cfg_path() -> io::Result<PathBuf> {
    let exec_path = std::env::current_exe()?;
    let exec_dir = exec_path
        .parent()
        .expect("executable must be in some directory");
    let mut cfg_dir = exec_dir.join("cfg");
    cfg_dir.push("v25_data_cfg.yml");
    Ok(cfg_dir)
}

const CLEANUP_DONE: &str = "V25Logs_cleaned.done";

fn main() -> io::Result<()> {
    let now = Instant::now();

    // get command line args
    let args = Args::parse();

    // cfg file path must be ./cfg/v25_data_cfg.yml, rel. to directory of executable
    let cfg_path = get_cfg_path()?;
    let cfg = &load_yml(&cfg_path)[0];

    // make sure that all commands such as ../ are resolved:
    let basepath = fs::canonicalize(args.dirname.clone())?;

    println!("cleaning files in {:?}", basepath);

    let cleaned_identifier = [args.dirname, CLEANUP_DONE.to_string()]
        .iter()
        .collect::<PathBuf>();

    // if cleaning is not forced, check if the directory was cleaned before
    if !args.force {
        if cleaned_identifier.is_file() {
            println!("cleanup was already done, found file '{CLEANUP_DONE}' :)");
            return Ok(());
        }
    }

    // collect all files in specified directory
    let entries: Vec<PathBuf> = fs::read_dir(basepath)?
        .into_iter()
        .filter(|r| r.is_ok()) // Get rid of Err variants for Result<DirEntry>
        .map(|r| r.unwrap().path()) // This is safe, since we only have the Ok variants
        .filter(|r| r.is_file()) // Filter out directories
        .collect();

    for file_path in entries.iter() {
        // >>> check #1
        // make sure the file has an extension and it is defined in config file
        let mut file_ext = String::new();
        match file_path.extension() {
            None => {
                if args.verbose {
                    println!("nok: {:?}\n  has no extension -> delete file", file_path)
                };
                fs::remove_file(file_path)?;
                continue;
            }
            Some(ext) => match ext.to_ascii_uppercase().to_str() {
                Some("") => {
                    if args.verbose {
                        println!("nok: {:?}\n  has no extension -> delete file", file_path)
                    };
                    fs::remove_file(file_path)?;
                    continue;
                }
                Some(other_str) => {
                    if cfg[other_str].is_badvalue() {
                        if args.verbose {
                            println!("unknown file extension '{other_str}', skipping");
                            continue;
                        }
                    } else {
                        // file extension was found in config, so set file_ext
                        file_ext = other_str.to_owned();
                    }
                }
                None => {
                    if args.verbose {
                        println!(
                            "! unexpected fail during file extension analysis, skipping {:?}",
                            file_path
                        );
                    };
                    continue;
                }
            },
        }
        file_ext = file_ext.to_ascii_uppercase();
        // <<< check 1 done.

        // load file content to a vector of strings
        let mut content = lines_from_file(file_path)?;

        let mut write: bool = false;

        // check #2
        // remove all empty strings at the end of content (trailing newlines)
        while content.last() == Some(&"".to_owned()) {
            if args.verbose {
                println!("nok: {:?}\n  last line is empty -> remove line", file_path)
            };
            content.pop();
            write = true;
        }

        // depending on the file extension, determine minimum number of lines.
        // the default is 2:
        let mut min_len = 2;
        // file_ext will only be set if it is defined in cfg yml.
        match cfg[file_ext.as_str()]["min_n_lines"].as_i64() {
            Some(n) => min_len = n as usize,
            None => {
                println!(
                "nok: {:?}:\n  failed to obtain minimum number of lines from cfg file; defaulting to {min_len}", file_path
            )
            }
        }

        if content.len() < min_len {
            if args.verbose {
                println!(
                    "nok: {:?}\n  has less than the minimum {min_len} lines -> delete file",
                    file_path
                )
            };
            fs::remove_file(file_path)?;
            continue; // these files should be deleted, so we can skip further tests
        }
        // <<< check 2 done.

        // >>> check #3
        // determine number of columns based on the first line (column header),
        // and the first line of data. Those must be equal.
        let n_col_header = n_data_fields(&content[min_len - 2], "\t");
        let n_col_data = n_data_fields(&content[min_len - 1], "\t");
        if n_col_data != n_col_header {
            if args.verbose {
                println!(
                    "nok: {:?}\n  has invalid number of fields in first line of data -> delete file",
                    file_path
                )
            };
            fs::remove_file(file_path)?;
            continue;
        }
        // <<< check 3 done.

        // >>> check #4.1
        // check number of fields in last line, must be the same as column header
        let n_col_data = n_data_fields(&content[content.len() - 1], "\t");
        if n_col_data != n_col_header {
            if args.verbose {
                println!(
                    "nok: {:?}\n  {n_col_data} field(s) in last line of data but header has {n_col_header} -> remove line",
                    file_path
                )
            };
            content.pop(); // coming from #3, if we pop one line, we still have at least one line of data
            write = true;
        }
        // <<< check 4.1 done.

        // >>> check #4.2
        // check the last field of the last line. assume that the line is
        // corrupted if that field has less characters than the last field
        // of the preceeding line.
        // this can only be done if there are at least two lines of data.
        if content.len() > min_len {
            let have = n_chars_last_field(&content[content.len() - 1], "\t").unwrap();
            let want = n_chars_last_field(&content[content.len() - 2], "\t").unwrap();
            if have < want {
                if args.verbose {
                    println!(
                        "nok: {:?}\n  last field of last line has {have} character(s), but want {want} -> remove line",
                        file_path
                    )
                };
                content.pop();
                write = true;
            }
        }
        // <<< check 4.2 done.

        // >>> check #5
        // after removing the last line again in #4.2, content could be too short...
        if content.len() < min_len {
            if args.verbose {
                println!(
                    "nok: {:?}\n  has less than the minimum {min_len} lines -> delete file",
                    file_path
                )
            };
            fs::remove_file(file_path)?;
            continue;
        }
        // <<< check 5 done.

        // all checked, write updated data back to file
        if file_ext.to_ascii_uppercase() == "OSC" {
            // special case: oscar / chemiluminescence detector files.
            lazy_static! { // use lazy_static to avoid regex compilation in each loop iteration
                static ref RE_DT: Regex =
                    Regex::new(r"\d{2}\.\d{2}\.\d{2} \d{2}:\d{2}:\d{2}\.\d{2}").unwrap();
            }
            // check datetime format in first line of file,
            // also make sure the file has not been updated before
            let datetime = content[0].clone();
            if RE_DT.is_match(datetime.as_str()) && !content[4].contains("DateTime") {
                // update header line and write to file
                content[4] = "\tDateTime".to_string() + content[4].clone().as_str();
                write_osc(file_path, content, 5, &datetime)?;
            }
        } else if write {
            lines_to_file(file_path, content)?;
        }

        // // write false and not an oscar file:
        // if args.verbose {
        //     println!("ok:  {:?}", file_path)
        // }
    }

    // dump an empty file after all files were cleaned
    let _ = fs::File::create(cleaned_identifier);

    let elapsed = now.elapsed();
    println!("updated {} files in {:.2?}", entries.len(), elapsed);
    Ok(())
}
