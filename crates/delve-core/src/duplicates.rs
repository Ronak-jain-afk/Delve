use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use rayon::prelude::*;
use tree_sitter::Node;
use yansi::Paint;

use crate::parser;

const WINDOW_SIZE: usize = 15;

#[derive(Debug, Clone)]
pub struct DupLocation {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone)]
pub struct DuplicateCluster {
    pub locations: Vec<DupLocation>,
    pub sample: String,
    pub token_count: usize,
}

pub fn tokenize_with_tree_sitter(file_path: &str, source: &str) -> Option<Vec<String>> {
    let lang = parser::language_for_file(file_path);
    let tree = match lang {
        "ts" | "tsx" => {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
                .ok()?;
            parser.parse(source, None)?
        }
        "js" => {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter_javascript::LANGUAGE.into())
                .ok()?;
            parser.parse(source, None)?
        }
        _ => return None,
    };

    let mut tokens = Vec::new();
    let root = tree.root_node();
    collect_normalized_tokens(root, source, &mut tokens);
    Some(tokens)
}

fn collect_normalized_tokens(node: Node, source: &str, tokens: &mut Vec<String>) {
    let is_named = node.is_named();

    if !is_named {
        let text = &source[node.byte_range()];
        let trimmed = text.trim();
        if !trimmed.is_empty() && !trimmed.chars().all(|c| c.is_ascii_whitespace()) {
            tokens.push(trimmed.to_string());
        }
        return;
    }

    let name = node.kind();
    if node.child_count() == 0 {
        match name {
            "identifier" => tokens.push("$id".to_string()),
            "string" | "string_fragment" => tokens.push("$str".to_string()),
            "number" => tokens.push("$num".to_string()),
            "true" | "false" => tokens.push("$bool".to_string()),
            "null" | "undefined" => tokens.push("$nil".to_string()),
            _ => {
                let text = &source[node.byte_range()];
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    tokens.push(trimmed.to_string());
                }
            }
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_normalized_tokens(child, source, tokens);
    }
}

fn hash_window(window: &[String]) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    window.hash(&mut hasher);
    hasher.finish()
}

fn merge_overlapping(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

pub fn find_duplicates(files: &[String]) -> Vec<DuplicateCluster> {
    let file_tokens: Vec<(String, Vec<String>)> = files
        .par_iter()
        .filter_map(|file_path| {
            let source = std::fs::read_to_string(file_path).ok()?;
            let tokens = tokenize_with_tree_sitter(file_path, &source)?;
            if tokens.len() < WINDOW_SIZE {
                return None;
            }
            Some((file_path.clone(), tokens))
        })
        .collect();

    let mut hash_map: HashMap<u64, Vec<(usize, usize)>> = HashMap::new();

    for (file_idx, (_path, tokens)) in file_tokens.iter().enumerate() {
        if tokens.len() < WINDOW_SIZE {
            continue;
        }
        for window_start in 0..=(tokens.len() - WINDOW_SIZE) {
            let window = &tokens[window_start..window_start + WINDOW_SIZE];
            let hash = hash_window(window);
            hash_map
                .entry(hash)
                .or_default()
                .push((file_idx, window_start));
        }
    }

    let mut cluster_map: HashMap<u64, Vec<(usize, Vec<(usize, usize)>)>> = HashMap::new();

    for (hash, locations) in hash_map {
        let mut file_groups: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        for (file_idx, start) in locations {
            file_groups
                .entry(file_idx)
                .or_default()
                .push((start, start + WINDOW_SIZE));
        }

        let merged: Vec<(usize, Vec<(usize, usize)>)> = file_groups
            .into_iter()
            .filter_map(|(file_idx, ranges)| {
                let merged = merge_overlapping(ranges);
                if merged.is_empty() { return None; }
                Some((file_idx, merged))
            })
            .collect();

        if merged.len() >= 2 {
            cluster_map.insert(hash, merged);
        }
    }

    let mut clusters: Vec<DuplicateCluster> = Vec::new();

    for (_hash, file_blocks) in cluster_map {
        let first_entry = &file_blocks[0];
        let first_file_idx = first_entry.0;
        let first_block = &first_entry.1[0];
        let sample_tokens = &file_tokens[first_file_idx].1[first_block.0..first_block.1];
        let sample = sample_tokens.join(" ");

        let mut dup_locations = Vec::new();
        for (file_idx, _blocks) in &file_blocks {
            let file_path = &file_tokens[*file_idx].0;
            dup_locations.push(DupLocation {
                file_path: file_path.clone(),
                start_line: 0,
                end_line: 0,
            });
        }

        let token_count: usize = file_blocks.iter().map(|(_, blocks)| {
            blocks.iter().map(|(s, e)| e - s).sum::<usize>()
        }).max().unwrap_or(WINDOW_SIZE);

        clusters.push(DuplicateCluster {
            locations: dup_locations,
            sample: if sample.len() > 120 {
                format!("{}...", &sample[..120])
            } else {
                sample
            },
            token_count,
        });
    }

    clusters
}

pub fn format_report(clusters: &[DuplicateCluster]) -> String {
    if clusters.is_empty() {
        return "  No duplicate blocks found.\n".to_string();
    }
    let mut output = format!("{}\n", Paint::yellow("DUPLICATE BLOCKS"));
    for (i, cluster) in clusters.iter().enumerate() {
        let loc_strs: Vec<String> = cluster
            .locations
            .iter()
            .map(|l| format!("  {}", l.file_path))
            .collect();
        output.push_str(&format!(
            "  {}. ({} tokens) {}\n    {}\n",
            i + 1,
            cluster.token_count,
            cluster.sample,
            loc_strs.join("\n    ")
        ));
    }
    output
}

pub fn run_dup(root: &Path, json: bool, config: &crate::config::DelveConfig) -> String {
    let progress = crate::progress::Progress::new(!json);
    progress.set_message("Parsing files...");
    let files = parser::find_source_files_with_ignore(root, &config.ignore);
    progress.set_message("Detecting duplicates...");
    let clusters = find_duplicates(&files);
    progress.finish();
    format_report(&clusters)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple() {
        let source = "function foo() { return 42; }";
        let tokens = tokenize_with_tree_sitter("test.ts", source);
        assert!(tokens.is_some(), "should produce tokens");
        assert!(!tokens.unwrap().is_empty(), "tokens should not be empty");
    }

    #[test]
    fn test_normalized_tokens() {
        let source = "const x = 1; const y = 2;";
        let tokens = tokenize_with_tree_sitter("test.ts", source).unwrap();
        let joined = tokens.join(" ");
        assert!(joined.contains("$id"), "identifiers should be normalized: {}", joined);
        assert!(joined.contains("$num"), "numbers should be normalized: {}", joined);
    }

    #[test]
    fn test_tokenize_jsx() {
        let source = "const el = <div>hello</div>;";
        let tokens = tokenize_with_tree_sitter("test.tsx", source);
        assert!(tokens.is_some(), "JSX should produce tokens");
    }
}
