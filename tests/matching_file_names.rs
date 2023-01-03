#[cfg(test)]
#[macro_use]
extern crate quickcheck;

#[cfg(test)]
mod tests {
    use git2::Repository;
    use quickcheck::{TestResult, Testable};
    use std::process::Command;
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
        let mut path = String::new();
        {
            let tmp = TemporaryRepository::new("https://github.com/alexcrichton/git2-rs");
            path = tmp.dir.path_str();
        }
        if !path.is_empty() {
            assert!(std::fs::read_dir(&path).is_err());
        }
    }

    #[test]
    #[should_panic]
    fn failed_clone_test() {
        TemporaryRepository::new("xxx https://github.com/alexcrichton/git2-rs");
    }

    fn run_model_grep(lookup_literal: &str, cwd: &str) -> String {
        let args = [
            "--fixed-strings", lookup_literal,
            "--files-with-matches",
            "--no-line-number",
            "--only-matching",
            "--no-ignore",
            "--color", "never",
            cwd
        ];
        String::from_utf8(
            Command::new("rg")
                .args(args)
                .output()
                .expect("failed to execute model grep process")
                .stdout,
        )
        .unwrap()
    }

    fn run_sut(lookup_literal: &str, cwd: &str) -> String {
        let args = [
            "--string", lookup_literal,
            "--directory", cwd,
            "--matching-files-only", "true"];
        String::from_utf8(
            Command::new(env!("CARGO_BIN_EXE_RR"))
                .args(args)
                .output()
                .expect("failed to execute model grep process")
                .stdout,
        )
        .unwrap()
    }

    #[test]
    fn rr_against_model_grep_pure_random() {
        use quickcheck::{quickcheck, TestResult};
        fn prop(varying: String) -> TestResult {
            if varying.contains('\0') || varying.trim().is_empty() {
                return TestResult::discard();
            }
            let lookup_string = format!("\"{}\"", varying);
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
            let lookup_string = format!("\"{}\"", keyword);
            let mut model = run_model_grep(&lookup_string, &dataset.dir.path_str())
                .split_ascii_whitespace()
                .map(|s| s.to_string())
                .collect::<Vec<String>>();
            model.sort();
            let mut sut = run_sut(&lookup_string, &dataset.dir.path_str())
                .split_ascii_whitespace()
                .map(|s| s.to_string())
                .collect::<Vec<String>>();
            sut.sort();
            assert_eq!(model, sut);
        }
    }
}
