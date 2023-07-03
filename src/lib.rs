use std::{io::Error, fs, str, path::{PathBuf, Path}, sync::OnceLock}; 
use regex::bytes::{Regex, RegexBuilder, Captures};

static RE: OnceLock<Regex> = OnceLock::new();

// TODO ASAP:
// - Right now we just blindly copy over files, but it would be massively
// beneficial to control exactly what files are copied, processed and left
// behind. For example:
//      - Process: .html .htmlraw
//      - Ignore: .htmlsnippet 
//      - and copy all others
// - Need custom error types. A library should never panic
// - Move tests to its own dir
// - Better tests

// TODO would be nice:
// - It would be nice if there was a cmdline util
// - It would be even nicer if you could specify a dir on a remote server to ssh
//   into and copy all files over to in one fell swoop
// - It would be nice if you could run it PHP style, with a dynamic server that
//   processes all files as they come, but this should only be used for testing
//   purposes, because the goal of this project is MAINLY for generating static
//   content easier

/// Processes all files in `source` and places the results into the dir in
/// `dest`. `source` can be either a file or a directory, but `dest` must only
/// be a directory.
///
/// # Examples
///
/// ```
/// use htmlprep::process;
/// use std::path::Path;
///
/// fn main() {
///     process("/var/www/staging", "/var/www/prod", "/");
///     // All files in staging will be copied to prod
/// }
/// ```
pub fn process(source: &str, dest: &str, root: &str) -> Result<(), Error> 
{
    // Do not let process overwrite anything
    let source = Path::new(source);
    let dest = Path::new(dest);
    let _root = Path::new(root);
    match dest.exists() {
        false => {
            fs::create_dir_all(dest)?;
        }
        true => {
            if !dest.is_dir() {
                panic!("Destination already exists and is not a dir. Refusing to overwrite file");
            }
        }
    };
    if source.is_dir() {
        process_dir(source, dest)?;
    } else if source.is_file() {
        process_file(source, &dest.join(source.file_name().unwrap()))?;
    }
    return Ok(())
}

/// Recursively tries to process every file in a directory
fn process_dir(dir: &Path, output_dir: &Path) -> Result<(), Error> {
    let dir_entries = dir.read_dir()?;
    for entry in dir_entries {
        let file = entry?;
        let file_type = file.file_type()?;
        let new_file = output_dir.join(file.file_name());
        if file_type.is_dir() {
            process_dir(&file.path(), &new_file)?; 
        } else if file_type.is_file() {
            process_file(&file.path(), &new_file)?;
        } else {
            panic!("Cannot parse simlinks");
        }
    }
    Ok(())
} 

/// Processes a single file
fn process_file(file: &Path, output_file: &Path) -> Result<(), Error> {
    let re = RE.get_or_init(|| RegexBuilder::new(r#"<!--.*?#include\s+"([^"]+)".*?-->"#)
        .dot_matches_new_line(true)
        .build()
        .unwrap()
    );

    let raw_html = std::fs::read(file)?;
    let mut processed_html = raw_html.clone();
    let captures: Vec<Captures>  = re.captures_iter(&raw_html).collect();
    for capture in captures.iter().rev() {
        let comment = unsafe{ capture.get(0).unwrap_unchecked() };
        let comment_path = unsafe{ capture.get(1).unwrap_unchecked() };
        let include_path = parse_path(&comment_path.as_bytes(), Path::new("./"), file.parent().unwrap()).unwrap();
        let include_contents = fs::read(include_path)?;
        let comment_range = comment.start()..comment.end();
        processed_html.splice(comment_range, include_contents);
    }

    fs::write(output_file, &processed_html)?;
    Ok(()) 
}

/// Web servers usually change their root dir before serving files. Thus, paths
/// in html files are likely to be based on a different root, however, this
/// library will probably be called by a user who has not changed their root.
/// Thus, this function is necessary to change the root of any absolute paths in
/// html files. 
fn parse_path(path_in_comment: &[u8], website_root: &Path, cwd: &Path) -> Result<Box<Path>, core::str::Utf8Error> {
    let path_as_str = str::from_utf8(path_in_comment)?;
    if path_as_str.starts_with('/') {
        return Ok(website_root.join(PathBuf::from(&path_as_str[1..])).into_boxed_path());
    } else {
        return Ok(cwd.join(PathBuf::from(&path_as_str)).into_boxed_path());
    }
}