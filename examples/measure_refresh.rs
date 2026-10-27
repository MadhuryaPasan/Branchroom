//! Reproducible, read-only refresh timing on a generated repository.
use branchroom::{git, process, snapshot};
fn main() {
    let root = std::env::temp_dir().join(format!("branchroom-benchmark-{}", snapshot::unique_id()));
    std::fs::create_dir(&root).unwrap();
    let _state = branchroom::settings::StateDirectory::enter_isolated(root.join(".qa-state"));
    git::git(&root, &["init", "-b", "main"]).unwrap();
    let mut stream = String::new();
    for n in 1..=200 {
        let message = format!("Commit {n}");
        stream.push_str(&format!("commit refs/heads/main\nmark :{n}\nauthor Benchmark <benchmark@example.invalid> {} +0000\ncommitter Benchmark <benchmark@example.invalid> {} +0000\ndata {}\n{}\nM 100644 inline file.txt\ndata {}\nversion {n}\n\n",1700000000+n,1700000000+n,message.len(),message,format!("version {n}\n").len()));
    }
    for n in 0..64 {
        stream.push_str(&format!(
            "reset refs/heads/topic-{n}\nfrom :{}\n\n",
            100 + n
        ));
        stream.push_str(&format!("commit refs/heads/diverged-{n}\nauthor Benchmark <benchmark@example.invalid> 1700000300 +0000\ncommitter Benchmark <benchmark@example.invalid> 1700000300 +0000\ndata 7\nFeature\nfrom :100\nM 100644 inline feature-{n}.txt\ndata 4\ntest\n\n"));
    }
    stream.push_str("done\n");
    process::input(
        "git",
        &root,
        &process::strings(&["fast-import", "--quiet"]),
        stream.as_bytes(),
        &[],
    )
    .unwrap();
    git::git(&root, &["reset", "--hard", "HEAD"]).unwrap();
    let mut samples = vec![];
    for _ in 0..3 {
        let before = process::command_count();
        let start = std::time::Instant::now();
        let state = git::load(&root, "main", 500).unwrap();
        samples.push(serde_json::json!({"milliseconds":start.elapsed().as_secs_f64()*1000.,"commands":process::command_count()-before,"branches":state.branches.len(),"commits":state.branch_commits.len()}));
    }
    println!("{}", serde_json::to_string_pretty(&samples).unwrap());
    // Only this executable's generated fixture may be removed.
    assert!(
        root.parent() == Some(std::env::temp_dir().as_path())
            && root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("branchroom-benchmark-")
    );
    let _ = std::fs::remove_dir_all(root);
}
