use std::path::PathBuf;
use tokio::sync::mpsc;
use walkdir::WalkDir;
use fs_extra::dir::get_size;
use clap::{Command, Arg};

fn format_size(size: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let (value, unit) = if size >= TB as u64 {
        (size as f64 / TB, "TB")
    } else if size >= GB as u64 {
        (size as f64 / GB, "GB")
    } else if size >= MB as u64 {
        (size as f64 / MB, "MB")
    } else if size >= KB as u64 {
        (size as f64 / KB, "kB")
    } else {
        (size as f64, "B")
    };

    format!("{:.3} {}", value, unit)
}

async fn find_largest_dirs(root_dir: PathBuf, tx: mpsc::Sender<(PathBuf, u64)>, max_depth: usize) {
    let entries = WalkDir::new(&root_dir)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| entry.file_type().is_dir());

    let mut tasks = Vec::new();

    for entry in entries {
        let tx = tx.clone();
        let dir = entry.path().to_path_buf();

        let task = tokio::spawn(async move {
            let size = get_size(&dir).unwrap(); // Calculate directory size using fs_extra
            
            tx.send((dir, size))
                .await
                .expect("Send error");
        });

        tasks.push(task);
    }

    for task in tasks {
        task.await.expect("Task error");
    }
}

#[tokio::main]
async fn main() {
    let matches = Command::new("Sort directories by size")
        .arg(Arg::new("max_recursion_depth")
            .short('r')
            .long("max-recursion")
            .value_name("DEPTH")
            .value_parser(clap::value_parser!(i64))
            .help("Set the maximum recursion depth")
            .required(false))
        .arg(Arg::new("directory")
            .value_name("DIRECTORY")
            .help("Target directory to scan")
            .required(true)
            .index(1))
        .get_matches();

    let max_depth = matches.get_one("max_recursion_depth")
        .map(|d: &i64| *d as usize)
        .unwrap_or(std::usize::MAX);

    let root_dir = PathBuf::from(
        matches.get_one::<String>("directory")
            .map(|s| s.as_str())
            .unwrap()
    );

    let (tx, mut rx) = mpsc::channel::<(PathBuf, u64)>(32);
    let largest_dirs = tokio::spawn(find_largest_dirs(root_dir.clone(), tx, max_depth));
    let mut results: Vec<(PathBuf, u64)> = Vec::new();

    while let Some(result) = rx.recv().await {
        results.push(result);
    }

    largest_dirs.await.expect("Task error");
    results.sort_by(|a, b| b.1.cmp(&a.1));

    for (i, (dir, size)) in results.iter().take(10).enumerate() {
        let formatted_size = format_size(*size);

        println!(
            "{:<2}: {:<40} - {}",
            i + 1,
            dir.display(),
            formatted_size
        );
    }
}

