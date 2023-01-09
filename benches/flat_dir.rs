#[cfg(test)]
mod benches {
    use tempfile::TempDir;
    use std::process::Command;

    use std::io::Write;
    use std::fs::File;
    use rand::prelude::*;
    use rand::distributions::Alphanumeric;

    pub trait PathStr {
        fn path_str(&self) -> String;
    }

    impl PathStr for TempDir {
        fn path_str(&self) -> String {
            self.path().to_string_lossy().to_owned().to_string()
        }
    }

    ///# Generate directory with files to test
    pub fn flat_dir() -> TempDir {

        let d = tempfile::tempdir().unwrap();
        for i in 0..5 {
            let file_path = d.path().join(format!("{}", i));
            let mut file = File::create(file_path.as_path()).unwrap();
            let mut rng = SmallRng::seed_from_u64(i);
            let data_size = rng.next_u64() as usize % 1024usize * 10usize;
            let data = rng.sample_iter(&Alphanumeric)
                        .take(data_size)
                        .map(char::from)
                        .collect::<String>();
            writeln!(file, "{}", data).unwrap();
        }
        d
    }    

    fn run_cmd(cmd: &str, args: Vec<&str>) {
            Command::new(cmd)
                .args(args)
                .output()
                .expect("failed to execute model grep process")
                .stdout;

    }

    pub fn run_model_grep(lookup_literal: &str, cwd: &str) {
        let args = vec![
            "--fixed-strings", lookup_literal,
            "--files-with-matches",
            "--no-line-number",
            "--only-matching",
            "--no-ignore",
            "--color", "never",
            cwd
        ];
        run_cmd("rg", args);
    }

    pub fn run_sut(lookup_literal: &str, cwd: &str) {
        let args = vec![
            "--string", lookup_literal,
            "--directory", cwd,
            "--matching-files-only", "true"];
        // In case of Github CI Action the environment variable is not set.
        // In production ready project the CI Actions would define the variable depending on the build type to mitigate the issue.
        // But in this playground project I don't care about learning CI Actions syntax.
        let cmd: &'static str = option_env!("CARGO_BIN_EXE_RR").unwrap_or("target/debug/rr");
        run_cmd(cmd, args);
    }

}


use brunch::Bench;
use benches::*;
use std::time::Duration;

brunch::benches!(
    Bench::new("sut::")
        .with_timeout(Duration::from_secs(60))
        .run_seeded_with(flat_dir, |vals| {
            run_sut("lookup_literal", &vals.path_str())
        }),
    Bench::new("model::")
        .with_timeout(Duration::from_secs(60))
        .run_seeded_with(flat_dir, |vals| {
            run_model_grep("lookup_literal", &vals.path_str())
        }),
);