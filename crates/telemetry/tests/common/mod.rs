use std::path::Path;

use serde_json::Value;

pub fn read_lines(log_dir: &Path) -> Vec<Value> {
    std::fs::read_dir(log_dir)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .flat_map(|contents| {
            contents
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect::<Vec<Value>>()
        })
        .collect()
}
