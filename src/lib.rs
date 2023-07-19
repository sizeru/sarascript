use std::{io, error, fmt, fs, str::{self, Utf8Error}, path::{PathBuf, Path}, sync::OnceLock, collections::HashMap}; 
use regex::bytes::{Regex, RegexBuilder, Captures};

static RE: OnceLock<Regex> = OnceLock::new();

#[derive(Debug)]
pub struct Error {
    kind: ErrorType,
    source: Option<Box<dyn error::Error>>,
}

#[derive(Debug)]
enum ErrorType {
    DirExists,
    IO,
    Utf8Parse,
    SimLinkFound,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error type: {:?} caused by: {:?} ", self.kind, self.source)
    }
}

impl error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error{kind: ErrorType::IO, source: Some(Box::new(error))}
    }
}

impl From<Utf8Error> for Error {
    fn from(error: Utf8Error) -> Self {
        Error{kind: ErrorType::Utf8Parse, source: Some(Box::new(error))}
    }
}


struct CompileOptions<'a> {
    process_extensions: HashMap<String, String>,
    skip_extensions: Vec<String>,
    source: &'a Path,
    dest: &'a Path,
    root: &'a Path,
}

impl Default for CompileOptions<'_> {
    fn default() -> Self {
        Self {
            process_extensions: HashMap::from([
                ("html".to_owned(), "html".to_owned()),
                ("htmlraw".to_owned(), "html".to_owned()),
            ]),
            skip_extensions: vec![
                String::from("htmlsnippet"),
                String::from("htmlprep"),
                String::from("")
            ],
            source: Path::new("."),
            dest: Path::new("processed_html"),
            root: Path::new("/"),
        }
    }
}

impl CompileOptions<'_> {
    pub fn compile(&self) -> Result<(), Error> 
    {
        // Do not allow dest dir to overwrite an existing file
        match fs::metadata(self.dest) {
            Err(error) => {
                if error.kind().eq(&io::ErrorKind::NotFound) {
                    fs::create_dir_all(self.dest)?;
                }
            },
            Ok(dest_metadata) => {
                if !dest_metadata.is_dir() {
                    return Err(Error{kind: ErrorType::DirExists, source: None});
                }
            }
        }
        // Ensure that the root dir is a dir and actually exists
        {
            let root_metadata = fs::metadata(self.root)?;
            if !root_metadata.is_dir() {
                return Err(Error{kind: ErrorType::DirExists, source: None});
            }
        }
        // All checkable options are valid. Begin processing.
        let source_metadata = fs::metadata(self.source)?;
        if source_metadata.is_dir() {
            self.compile_dir(self.source, self.dest)?;
        } else if source_metadata.is_file() {
            let processed = process_file(self.source, self.root)?;
            fs::write(
                self.dest.join(self.source.file_name().unwrap()),
                &processed
            )?;
        }
        Ok(())
    }

    /// Recursively tries to process every file in a directory
    fn compile_dir(&self, source: &Path, dest: &Path) -> Result<(), Error> {
        let dir_entries = source.read_dir()?;
        for entry in dir_entries {
            let file = entry?;
            let file_type = file.file_type()?;
            let file_path = file.path();
            let file_extension = match file_path.extension() {
                Some(extension) => extension.to_str().unwrap().to_owned(),
                None => String::from(""),
            };
            if self.skip_extensions.contains(&file_extension) {
                continue;
            }
            let dest = dest.join(&file.file_name());
            if file_type.is_dir() {
                self.compile_dir(&file_path, &dest)?; 
            } else if file_type.is_file() {
                match self.process_extensions.get(&file_extension) {
                    None => {
                        fs::copy(&file_path, &dest)?;
                    },
                    Some(extension) => {
                        let processed = process_file(&file.path(), self.root)?;
                        fs::write(&dest.with_extension(extension), &processed)?;
                    },
                }
            } else {
                return Err(Error{ kind: ErrorType::SimLinkFound, source: None });
            }
        }
        Ok(())
    } 

}

// TODO ASAP:
// - Better tests

// TODO would be nice:
// - It would be nice if there was a cmdline util
// - It would be even nicer if you could specify a dir on a remote server to ssh
//   into and copy all files over to in one fell swoop
// - It would be nice if you could run it PHP style, with a dynamic server that
//   processes all files as they come, but this should only be used for testing
//   purposes, because the goal of this project is MAINLY for generating static
//   content easier

/// Processes a single file and returns a structure containing the entire file
/// in memory
pub fn process_file(file: &Path, webroot: &Path) -> Result<Vec<u8>, Error> {
    let re = RE.get_or_init(|| RegexBuilder::new(
        r#"<!--.*?#include\s+"([^"]+)".*?-->"#
    )
        .dot_matches_new_line(true)
        .build()
        .unwrap()
    );

    let webroot = Path::new(webroot);
    let raw_html = std::fs::read(file)?;
    let mut processed_html = raw_html.clone();
    let captures: Vec<Captures>  = re.captures_iter(&raw_html).collect();
    for capture in captures.iter().rev() {
        let comment = unsafe{ capture.get(0).unwrap_unchecked() };
        let comment_path = unsafe{ capture.get(1).unwrap_unchecked() };
        let include_path = parse_path(
            &comment_path.as_bytes(),
            webroot, 
            unsafe { file.parent().unwrap_unchecked() }
        )?;
        let include_contents = fs::read(include_path)?;
        let comment_range = comment.start()..comment.end();
        processed_html.splice(comment_range, include_contents);
    }

    return Ok(processed_html);

}
/// Processes all files in `source` and places the results into the dir in
/// `dest`. `source` can be either a file or a directory, but `dest` must only
/// be a directory.
///
/// # Examples
///
/// ```
/// fn main() {
///     htmlprep::compile("/var/www/staging", "/var/www/prod", "/");
///     // All files in staging will be copied to prod
/// }
/// ```
pub fn compile(source: &str, dest: &str, webroot: &str) -> Result<(), Error> 
{
    let mut options = CompileOptions::default();
    options.source = Path::new(source);
    options.dest = Path::new(dest);
    options.root = Path::new(webroot);
    return options.compile();
}

/// Web servers usually change their root dir before serving files. Thus, paths
/// in html files are likely to be based on a different root, however, this
/// library will probably be called by a user who has not changed their root.
/// Thus, this function is necessary to change the root of any absolute paths in
/// html files. 
fn parse_path(path_in_comment: &[u8], website_root: &Path, cwd: &Path) -> Result<Box<Path>, core::str::Utf8Error> {
    let path_as_str = str::from_utf8(path_in_comment)?;
    if path_as_str.starts_with('/') {
        let x = Ok(website_root.join(PathBuf::from(&path_as_str[1..])).into_boxed_path());
        return x;
    } else {
        let x = Ok(cwd.join(PathBuf::from(&path_as_str)).into_boxed_path());
        return x;
    }
}