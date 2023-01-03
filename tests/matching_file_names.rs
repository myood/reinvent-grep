#[cfg(test)]
#[macro_use]
extern crate quickcheck;

#[cfg(test)]
mod tests {
    use std::process::Command;
    use git2::Repository;
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
        dir: tempfile::TempDir
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

    #[test]
    fn subprocess_test() {
        let output = if cfg!(target_os = "windows") {
            Command::new("cmd")
                    .args(["/C", "echo hello"])
                    .output()
                    .expect("failed to execute process")
        } else {
            Command::new("sh")
                    .arg("-c")
                    .arg("echo hello")
                    .output()
                    .expect("failed to execute process")
        };
        let output = String::from_utf8(output.stdout).unwrap();
        assert!("hello".to_string().trim() == output.trim());
    }
}