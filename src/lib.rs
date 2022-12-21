use std::{
    fs,
    io::{self, prelude::*, BufRead, Write},
    path::{Path, PathBuf},
};

use yaml_rust::YamlLoader;

/// load_yml loads a yaml file, used here to specifiy minimum number of lines per file type.
pub fn load_yml(filename: &PathBuf) -> Vec<yaml_rust::Yaml> {
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
pub fn lines_from_file(filename: impl AsRef<Path>) -> Result<Vec<String>, io::Error> {
    // this could return a file not found error:
    let file = fs::File::open(filename)?;
    let buf = io::BufReader::new(file);
    // can return an error if file is not readable:
    buf.lines().collect::<Result<Vec<String>, io::Error>>()
}

/// lines_to_file writes a vector of strings to a textfile. trims lines before write.
pub fn lines_to_file(filename: impl AsRef<Path>, content: Vec<String>) -> io::Result<()> {
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
pub fn write_osc(
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
pub fn n_data_fields(s: &String, delimiter: &str) -> usize {
    s.trim().split(delimiter).collect::<Vec<&str>>().len()
}

/// n_chars_last_field returns the number of characters found in the last field of a
/// delimited string.
pub fn n_chars_last_field(s: &String, delimiter: &str) -> Option<usize> {
    match s.trim().split(delimiter).collect::<Vec<&str>>().last() {
        Some(field) => Some(field.chars().count()),
        None => None,
    }
}

/// get_cfg_path returns the directory where the cfg file is expected
pub fn get_cfg_path() -> io::Result<PathBuf> {
    let exec_path = std::env::current_exe()?;
    let exec_dir = exec_path
        .parent()
        .expect("executable must be in some directory");
    let mut cfg_dir = exec_dir.join("cfg");
    cfg_dir.push("v25_data_cfg.yml");
    Ok(cfg_dir)
}
