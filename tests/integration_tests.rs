// use htmlprep::*;
// use tempdir::TempDir;

// const ROOT: &str = "testdata";

// fn compile_file_test(file: &str) -> Result<(), Box<dyn std::error::Error>> {
//     let tempdir = TempDir::new("htmlprep-test").unwrap();
//     let _ = compile(&format!("{ROOT}/{file}"), tempdir.into_path().to_str().unwrap(), ROOT)?;
//     return Ok(());
// }

// #[test]
// fn needle_in_haystack() {
//     compile_file_test("needle_in_haystack.html").unwrap();
// }

// #[test]
// fn bad_format() {
//     compile_file_test("bad_format1.html").unwrap(); 
// }

// #[test]
// fn two_includes() {
//     compile_file_test("two_includes.html").unwrap();
// }

// #[test]
// fn two_includes2() {
//     compile_file_test("two_includes2.html").unwrap();
// }

// #[test]
// fn entire_test_dir() {
//     let tempdir = TempDir::new("htmlprep-test").unwrap();
//     compile(ROOT, &tempdir.into_path().to_str().unwrap(), ROOT).unwrap();
// }