#[cfg(test)]
#[macro_use]
extern crate quickcheck;

#[cfg(test)]
mod tests {
    use git2::Repository;
    use std::{process::Command, collections::HashSet};
    use tempfile::TempDir;

    quickcheck! {
        fn environment_test(xs: bool) -> bool {
            xs == !(!xs)
        }
    }

    trait PathStr {
        fn path_str(&self) -> String;
    }

    impl PathStr for TempDir {
        fn path_str(&self) -> String {
            self.path().to_string_lossy().to_owned().to_string()
        }
    }

    struct TemporaryRepository {
        dir: tempfile::TempDir,
    }

    impl TemporaryRepository {
        fn new(url: &'static str) -> TemporaryRepository {
            let td = tempfile::tempdir().unwrap();
            let path = td.path_str();
            let _ = Repository::clone(url, &path).unwrap();
            assert!(std::fs::read_dir(&path).is_ok());
            TemporaryRepository { dir: td }
        }
    }

    #[test]
    fn clone_test() {
        let mut path = Option::None;
        {
            let tmp = TemporaryRepository::new("https://github.com/alexcrichton/git2-rs");
            path = Option::Some(tmp.dir.path_str());
        }
        if let Some(path) = path {
            assert!(std::fs::read_dir(&path).is_err());
        }
    }

    #[test]
    #[should_panic]
    fn failed_clone_test() {
        TemporaryRepository::new("xxx https://github.com/alexcrichton/git2-rs");
    }
    
    fn run_cmd(cmd: &str, args: Vec<&str>) -> HashSet<String> {
        let cmd_out = Command::new(cmd)
            .args(args)
            .output()
            .expect("failed to execute model grep process")
            .stdout;
        let rv = String::from_utf8(cmd_out)
        .unwrap()        
        .split("\n")
        .map(|s| s.to_string())
        .collect::<HashSet<String>>();
        rv
    }

    fn run_model_grep(lookup_literal: &str, cwd: &str) -> HashSet<String> {
        let args = vec![
            "--fixed-strings", lookup_literal,
            "--files-with-matches",
            "--color", "never",
            cwd
        ];
        run_cmd("rg", args)
    }

    fn run_sut(lookup_literal: &str, cwd: &str) -> HashSet<String> {
        let args = vec![
            "--string", lookup_literal,
            "--directory", cwd,
            "--matching-files-only", "true"];
        // In case of Github CI Action the environment variable is not set.
        // In production ready project the CI Actions would define the variable depending on the build type to mitigate the issue.
        // But in this playground project I don't care about learning CI Actions syntax.
        let cmd: &'static str = option_env!("CARGO_BIN_EXE_RR").unwrap_or("target/debug/rr");
        run_cmd(cmd, args)
    }

    #[test]
    fn rr_against_model_grep_pure_random() {
        use quickcheck::{quickcheck, TestResult};
        fn prop(varying: String) -> TestResult {
            if varying.contains('\0') || varying.trim().is_empty() {
                return TestResult::discard();
            }
            let lookup_string = format!("\"{}\"", varying.to_ascii_lowercase());
            let sut = run_sut(&lookup_string, "src");
            let model = run_model_grep(&lookup_string, "src");
            return TestResult::from_bool(model == sut);
        }
        quickcheck(prop as fn(String) -> TestResult);
    }

    #[test]
    fn rr_against_model_grep_rust_keywords() {
        let keywords = [
            "as",
            "use",
            "extern crate",
            "break",
            "const",
            "continue",
            "crate",
            "else",
            "if",
            "if let",
            "enum",
            "extern",
            "false",
            "fn",
            "for",
            "if",
            "impl",
            "in",
            "for",
            "let",
            "loop",
            "match",
            "mod",
            "move",
            "mut",
            "pub",
            "impl",
            "ref",
            "return",
            "Self",
            "self",
            "static",
            "struct",
            "super",
            "trait",
            "true",
            "type",
            "unsafe",
            "use",
            "where",
            "while",
            "abstract",
            "alignof",
            "become",
            "box",
            "do",
            "final",
            "macro",
            "offsetof",
            "override",
            "priv",
            "proc",
            "pure",
            "sizeof",
            "typeof",
            "unsized",
            "virtual",
            "yield",
        ];
        
        let dataset = TemporaryRepository::new("https://github.com/alexcrichton/git2-rs");
        
        for keyword in keywords {
            let lookup_string = keyword;
            // For unknown reason rg does not match LICENSE files in git2-rs/git2-url and alike
            // so clone src directory where only *.rs files are present 
            let cwd = dataset.dir.path().join("src").to_string_lossy().to_string();

            let model = run_model_grep(&lookup_string, &cwd);
            let sut = run_sut(&lookup_string, &cwd);
            
            if model != sut {
                let difference: Vec<_> = model.symmetric_difference(&sut).collect();
                assert_eq!(vec![""], difference);
            }
        }
    }
}
