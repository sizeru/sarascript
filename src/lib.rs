use std::{io, error, fmt, fs, str::{self, Utf8Error}, path::{PathBuf, Path}, sync::OnceLock, collections::HashMap}; 
use regex::bytes::{Regex, RegexBuilder, Captures};

static RE_INCLUDE: OnceLock<Regex> = OnceLock::new();
const INCLUDE_REGEX: &str = r##"<!--\s*?#include\s+"([^"]+)"\s*?-->"##;
static RE_PLACEHOLDER: OnceLock<Regex> = OnceLock::new();
const PLACEHOLDER_REGEX: &str = r##"<!--\s*?#placeholder\s+"([^"]+)"\s*?-->"##;

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
    NoFilename,
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

#[derive(Debug)]
pub struct Html {
    bytes: Vec<u8>
}

impl From<Html> for Vec<u8> {
    fn from(value: Html) -> Self {
        return value.bytes;
    }
}

impl From<Vec<u8>> for Html {
    fn from(value: Vec<u8>) -> Self {
        return Html { bytes: value };
    }
}

pub struct CompileOptions<'a> {
    pub process_extensions: HashMap<String, String>,
    pub skip_extensions: Vec<String>,
    pub source: &'a Path,
    pub dest: &'a Path,
    pub root: &'a Path,
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

    /// Recursively tries to process every file in a directory. Intentionally
    /// left private.
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
    if file.file_name().is_none() {
        return Err(Error {kind: ErrorType::NoFilename, source: None });
    }

    let raw_html = fs::read(file)?;
    return Ok(Html::process(raw_html.as_slice(), webroot, unsafe{file.parent().unwrap_unchecked()} 
        /* a file with a name is guaranteed to also have a parent */
    )?.into());
}

impl Html {
    /// Pre-processes a slice of bytes.
    pub fn process(html: &[u8], website_root: &Path, cwd: &Path) -> Result<Self, Error> {
        let re_include = RE_INCLUDE.get_or_init(|| RegexBuilder::new(INCLUDE_REGEX)
            .dot_matches_new_line(true)
            .build()
            .unwrap()
        );
        let mut processed_html = html.to_vec();
        let include_captures: Vec<Captures>  = re_include.captures_iter(&html).collect();
        for capture in include_captures.iter().rev() {
            let comment = unsafe{ capture.get(0).unwrap_unchecked() };
            let comment_path = unsafe{ capture.get(1).unwrap_unchecked() };
            let include_path = make_path_absolute(&comment_path.as_bytes(), website_root, cwd)?;
            let include_contents = fs::read(include_path)?;
            let comment_range = comment.start()..comment.end();
            processed_html.splice(comment_range, include_contents);
        }
    
        return Ok(processed_html.into());
    }

    /// Returns all `placeholders` in the file. A `placeholder` is a range in
    /// the file which is meant to be replaced server-side each time the file is
    /// requested. A `placeholder` is defined in an html file using a
    /// `#placeholder` comment. This allows for arbitrary insertion of HTML at
    /// runtime. The order of replacement does not matter.
    /// 
    /// # Examples
    /// 
    /// ```
    /// use std::path::Path;
    /// use std::str;
    /// use htmlprep::*;
    /// 
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let raw_html = r##"
    ///             <!DOCTYPE html><html><body>
    ///                 <!-- #placeholder "name" -->
    ///                 <!-- #placeholder "visitor-number" -->
    ///             </body></html>"##.as_bytes();
    ///     
    ///     let mut html: Html = raw_html.to_vec().into();
    ///     let mut placeholders = html.get_placeholders()?;
    ///     assert!(placeholders.contains("name"));
    ///     assert!(placeholders.contains("visitor-number"));
    ///     
    ///     let name = "Alice";
    ///     let name_replacement = format!("<p>Welcome to the site, <b>{name}!</b></p>");
    ///     html.replace_placeholder(&mut placeholders, "name", name_replacement.as_bytes());
    ///     let visitor_number = 1234;
    ///     let visitor_num_replacement = format!("<p>You are visitor number: {visitor_number}</p>");
    ///     html.replace_placeholder(&mut placeholders, "visitor-number", visitor_num_replacement.as_bytes());
    ///     // Calling this function again is a no-op
    ///     html.replace_placeholder(&mut placeholders, "visitor-number", visitor_num_replacement.as_bytes());
    ///     
    ///     let html_vec: Vec<u8> = html.into();
    ///     let result = r##"
    ///             <!DOCTYPE html><html><body>
    ///                 <p>Welcome to the site, <b>Alice!</b></p>
    ///                 <p>You are visitor number: 1234</p>
    ///             </body></html>"##.as_bytes().to_vec();
    ///     
    ///     assert!(result.eq(&html_vec));
    ///     return Ok(());
    /// }
    /// ```
    pub fn get_placeholders(&self) -> Result<Placeholders, Error> {
        let re_placeholder = RE_PLACEHOLDER.get_or_init(|| RegexBuilder::new(PLACEHOLDER_REGEX)
            .dot_matches_new_line(true)
            .build()
            .unwrap()
        );

        let mut placeholders = Vec::new();
        for capture in  re_placeholder.captures_iter(&self.bytes) {
            let comment = unsafe{ capture.get(0).unwrap_unchecked() };
            let placeholder_name = unsafe{ capture.get(1).unwrap_unchecked() };
            let name = str::from_utf8(placeholder_name.as_bytes())?;
            placeholders.push(
                Placeholder {
                    start: comment.start(),
                    end: comment.end(),
                    name: name.to_owned(),
                }
            )
        }
        
        return Ok(placeholders.into());
    }

    /// Replaces the `placeholder_name` placeholder in the calling Html struct
    /// with `replacement`. Upon completion of this function, the replaced
    /// Placeholder will be removed from `placeholders`. 
    pub fn replace_placeholder(&mut self, placeholders: &mut Placeholders, placeholder_name: &str, replacement: &[u8]) {
        // Placeholders are kept in sorted order so that only what's necessary to update can be updated 
        if let Some(index) = placeholders.data.iter().position(|p| p.name.eq(placeholder_name)) {
            let to_be_replaced = placeholders.data.remove(index);
            let bytes_added: isize = replacement.len() as isize - (to_be_replaced.end - to_be_replaced.start) as isize;
            for i in index..placeholders.data.len() {
                let placeholder = placeholders.data.get_mut(i).unwrap();
                placeholder.start = (placeholder.start as isize + bytes_added) as usize;
                placeholder.end = (placeholder.end as isize + bytes_added) as usize;
            }
            self.bytes.splice(to_be_replaced.start..to_be_replaced.end, replacement.to_vec());
        }
    }
}

#[derive(Debug)]
pub struct Placeholders {
    data: Vec<Placeholder>
}

impl Placeholders {
    pub fn contains<T>(&self, value: &T) -> bool
    where
        T: ?Sized, 
        Placeholder: PartialEq<T>,
    {
        self.data.iter().any(|val| val == value)
    }
}

impl PartialEq<str> for Placeholder {
    fn eq(&self, other: &str) -> bool {
        self.name == other
    }
}

impl From<Vec<Placeholder>> for Placeholders {
    fn from(value: Vec<Placeholder>) -> Self {
        Self { data: value }
    }
}

#[derive(Debug, PartialEq)]
pub struct Placeholder {
    start: usize,
    end: usize,
    name: String,
}



/// Processes all files in `source` and places the results into the dir in
/// `dest`. `source` can be either a file or a directory, but `dest` must only
/// be a directory. Processing means that all #include comments in the source
/// html are replaced with the file specified in the comment. Processing will
/// not replace #placeholder comments, as these are mean to be replaced
/// dynamically each time the file is requested. 
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
fn make_path_absolute(path_in_comment: &[u8], website_root: &Path, cwd: &Path) -> Result<Box<Path>, core::str::Utf8Error> {
    let path_as_str = str::from_utf8(path_in_comment)?;
    if path_as_str.starts_with('/') {
        let x = Ok(website_root.join(PathBuf::from(&path_as_str[1..])).into_boxed_path());
        return x;
    } else {
        let x = Ok(cwd.join(PathBuf::from(&path_as_str)).into_boxed_path());
        return x;
    }
}