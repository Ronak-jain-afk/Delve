use std::collections::{HashMap, HashSet, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::Path;

use rayon::prelude::*;
use tree_sitter::Node;
use yansi::Paint;

use crate::parser;

const WINDOW_SIZE: usize = 25;
const NGRAM_N: usize = 3;
const MINHASH_K: usize = 80;
const LSH_BANDS: usize = 16;
const LSH_ROWS: usize = 5;

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
    pub is_near_dup: bool,
}

#[derive(Debug, Clone)]
struct NearWindow {
    file_idx: usize,
    window_start: usize,
    trigrams: Vec<u64>,
    band_keys: Vec<u64>,
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

fn minhash_hash(x: u64, seed: u64) -> u64 {
    let mut h = x.wrapping_add(seed);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;
    h
}

fn ngram_hashes(tokens: &[String]) -> Vec<u64> {
    tokens.windows(NGRAM_N).map(|w| {
        let mut hasher = std::hash::DefaultHasher::new();
        for t in w {
            t.hash(&mut hasher);
        }
        hasher.finish()
    }).collect()
}

fn minhash_signature(shingles: &[u64]) -> Vec<u64> {
    (0..MINHASH_K).map(|i| {
        let seed = (i as u64).wrapping_mul(0x9e3779b97f4a7c15);
        shingles.iter()
            .map(|&s| minhash_hash(s, seed))
            .min()
            .unwrap_or(u64::MAX)
    }).collect()
}

fn band_hashes(sig: &[u64]) -> Vec<u64> {
    sig.chunks(LSH_ROWS).map(|band| {
        let mut h = 0u64;
        for &v in band {
            h = h.wrapping_mul(31).wrapping_add(v);
        }
        h
    }).collect()
}

fn jaccard_similarity(a: &[u64], b: &[u64]) -> f64 {
    let set_a: BTreeSet<u64> = a.iter().copied().collect();
    let set_b: BTreeSet<u64> = b.iter().copied().collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.len() + set_b.len() - intersection;
    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
}

fn union_find_find(parent: &mut Vec<usize>, x: usize) -> usize {
    if parent[x] != x {
        parent[x] = union_find_find(parent, parent[x]);
    }
    parent[x]
}

fn union_find_union(parent: &mut Vec<usize>, x: usize, y: usize) {
    let px = union_find_find(parent, x);
    let py = union_find_find(parent, y);
    if px != py {
        parent[py] = px;
    }
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

pub fn tokenize_files(files: &[String]) -> Vec<(String, Vec<String>)> {
    files
        .par_iter()
        .filter_map(|file_path| {
            let source = std::fs::read_to_string(file_path).ok()?;
            let tokens = tokenize_with_tree_sitter(file_path, &source)?;
            if tokens.len() < WINDOW_SIZE {
                return None;
            }
            Some((file_path.clone(), tokens))
        })
        .collect()
}

pub fn find_duplicates(file_tokens: &[(String, Vec<String>)]) -> Vec<DuplicateCluster> {

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
            is_near_dup: false,
        });
    }

    clusters
}

pub fn find_near_duplicates(
    file_tokens: &[(String, Vec<String>)],
    threshold: f64,
) -> Vec<DuplicateCluster> {
    if threshold >= 1.0 {
        return Vec::new();
    }

    let windows: Vec<NearWindow> = file_tokens
        .iter()
        .enumerate()
        .flat_map(|(file_idx, (_path, tokens))| {
            if tokens.len() < WINDOW_SIZE {
                return Vec::new();
            }
            (0..=(tokens.len() - WINDOW_SIZE)).map(move |window_start| {
                let window = &tokens[window_start..window_start + WINDOW_SIZE];
                let trigrams = ngram_hashes(window);
                let sig = minhash_signature(&trigrams);
                let band_keys = band_hashes(&sig);
                NearWindow { file_idx, window_start, trigrams, band_keys }
            }).collect()
        })
        .collect();

    if windows.len() < 2 {
        return Vec::new();
    }

    let mut parent: Vec<usize> = (0..windows.len()).collect();
    let mut compared: HashSet<(usize, usize)> = HashSet::new();
    let mut bucket_map: HashMap<u64, Vec<usize>>;

    for band_idx in 0..LSH_BANDS {
        bucket_map = HashMap::new();
        for (win_idx, w) in windows.iter().enumerate() {
            bucket_map.entry(w.band_keys[band_idx]).or_default().push(win_idx);
        }

        for bucket in bucket_map.values() {
            if bucket.len() < 2 { continue; }
            for i in 0..bucket.len() {
                for j in (i + 1)..bucket.len() {
                    let a = bucket[i];
                    let b = bucket[j];
                    if a == b { continue; }
                    let key = if a < b { (a, b) } else { (b, a) };
                    if !compared.insert(key) { continue; }

                    let wa = &windows[a];
                    let wb = &windows[b];
                    if wa.file_idx == wb.file_idx {
                        let gap = if wa.window_start > wb.window_start {
                            wa.window_start - wb.window_start
                        } else {
                            wb.window_start - wa.window_start
                        };
                        if gap < WINDOW_SIZE { continue; }
                    }

                    let sim = jaccard_similarity(&wa.trigrams, &wb.trigrams);
                    if sim >= threshold {
                        union_find_union(&mut parent, a, b);
                    }
                }
            }
        }
    }

    for i in 0..windows.len() {
        union_find_find(&mut parent, i);
    }

    let mut groups: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
    for (win_idx, w) in windows.iter().enumerate() {
        let root = parent[win_idx];
        groups.entry(root).or_default().push((w.file_idx, w.window_start));
    }

    groups.into_values()
        .filter(|locations| {
            let distinct: HashSet<&(usize, usize)> = locations.iter().collect();
            distinct.len() >= 2 && distinct.iter().map(|(fi, _)| fi).collect::<HashSet<_>>().len() >= 1
        })
        .map(|locations| {
            let distinct: Vec<(usize, usize)> = {
                let mut seen = HashSet::new();
                locations.into_iter().filter(|loc| seen.insert(*loc)).collect()
            };
            let first = distinct[0];
            let sample_tokens = &file_tokens[first.0].1[first.1..first.1 + WINDOW_SIZE];
            let sample = sample_tokens.join(" ");

            let dup_locations: Vec<DupLocation> = distinct.iter()
                .map(|(fi, _)| DupLocation {
                    file_path: file_tokens[*fi].0.clone(),
                    start_line: 0,
                    end_line: 0,
                })
                .collect();

            DuplicateCluster {
                locations: dup_locations,
                sample: if sample.len() > 120 {
                    format!("{}...", &sample[..120])
                } else {
                    sample
                },
                token_count: WINDOW_SIZE,
                is_near_dup: true,
            }
        })
        .collect()
}

pub fn format_report(clusters: &[DuplicateCluster]) -> String {
    let exact: Vec<&DuplicateCluster> = clusters.iter().filter(|c| !c.is_near_dup).collect();
    let near: Vec<&DuplicateCluster> = clusters.iter().filter(|c| c.is_near_dup).collect();

    if exact.is_empty() && near.is_empty() {
        return "  No duplicate blocks found.\n".to_string();
    }

    let mut output = String::new();

    if !exact.is_empty() {
        output.push_str(&format!("{}\n", Paint::yellow("DUPLICATE BLOCKS")));
        for (i, cluster) in exact.iter().enumerate() {
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
    }

    if !near.is_empty() {
        output.push_str(&format!("{}\n", Paint::cyan("NEAR-DUPLICATE BLOCKS (Jaccard ≥ 0.7)")));
        for (i, cluster) in near.iter().enumerate() {
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
    }

    output
}

pub fn run_dup(root: &Path, json: bool, config: &crate::config::DelveConfig) -> crate::CommandResult {
    let progress = crate::progress::Progress::new(!json);
    progress.set_message("Parsing files...");
    let files = parser::find_source_files_with_ignore(root, &config.ignore);
    progress.set_message("Tokenizing...");
    let file_tokens = tokenize_files(&files);
    let threshold = config.thresholds.jaccard_threshold;
    progress.set_message("Detecting exact duplicates...");
    let exact = find_duplicates(&file_tokens);
    progress.set_message("Detecting near-duplicates...");
    let near = find_near_duplicates(&file_tokens, threshold);
    progress.finish();
    let mut all = exact;
    all.extend(near);
    let output = format_report(&all);
    let exit_code = if all.is_empty() { 0 } else { 1 };
    crate::CommandResult { output, exit_code, score: if all.is_empty() { 100 } else { 0 } }
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

    #[test]
    fn test_jaccard_similarity_identical() {
        let a = vec![1, 2, 3, 4, 5];
        let b = vec![1, 2, 3, 4, 5];
        let sim = jaccard_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6, "identical sets should have similarity 1.0");
    }

    #[test]
    fn test_jaccard_similarity_disjoint() {
        let a = vec![1, 2, 3];
        let b = vec![4, 5, 6];
        let sim = jaccard_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-6, "disjoint sets should have similarity 0.0");
    }

    #[test]
    fn test_jaccard_similarity_half() {
        let a = vec![1, 2];
        let b = vec![1];
        let sim = jaccard_similarity(&a, &b);
        assert!((sim - 0.5).abs() < 1e-6, "should have similarity 0.5, got {}", sim);
    }

    #[test]
    fn test_ngram_hashes() {
        let tokens: Vec<String> = vec!["a", "b", "c", "d", "e"].into_iter().map(|s| s.to_string()).collect();
        let hashes = ngram_hashes(&tokens);
        assert_eq!(hashes.len(), 3, "5 tokens should produce 3 trigrams");
    }

    #[test]
    fn test_minhash_signature_length() {
        let shingles = vec![10, 20, 30, 40, 50];
        let sig = minhash_signature(&shingles);
        assert_eq!(sig.len(), MINHASH_K, "signature length should match MINHASH_K");
    }

    #[test]
    fn test_band_hashes_length() {
        let sig: Vec<u64> = (0..MINHASH_K as u64).collect();
        let bands = band_hashes(&sig);
        assert_eq!(bands.len(), LSH_BANDS, "number of bands should match LSH_BANDS");
    }

    #[test]
    fn test_find_near_duplicates_detects_similar() {
        let mut tokens_a = Vec::new();
        let mut tokens_b = Vec::new();
        for i in 0..30 {
            tokens_a.push(format!("token{}", i));
            tokens_b.push(if i >= 25 {
                format!("other{}", i)
            } else {
                format!("token{}", i)
            });
        }
        assert!(tokens_a.len() >= WINDOW_SIZE, "need at least {} tokens", WINDOW_SIZE);
        let file_tokens = vec![
            ("a.ts".to_string(), tokens_a),
            ("b.ts".to_string(), tokens_b),
        ];
        let clusters = find_near_duplicates(&file_tokens, 0.5);
        assert!(!clusters.is_empty(), "should detect near-duplicates between similar token sequences");
    }

    #[test]
    fn test_find_near_duplicates_empty_for_dissimilar() {
        let tokens_a: Vec<String> = (0..30).map(|i| format!("aa{}", i)).collect();
        let tokens_b: Vec<String> = (0..30).map(|i| format!("bb{}", i)).collect();
        let file_tokens = vec![
            ("a.ts".to_string(), tokens_a),
            ("b.ts".to_string(), tokens_b),
        ];
        let clusters = find_near_duplicates(&file_tokens, 0.9);
        assert!(clusters.is_empty(), "dissimilar code should not be near-duplicates at high threshold");
    }

    #[test]
    fn test_near_duplicates_off_for_threshold_1() {
        let tokens_a: Vec<String> = (0..30).map(|i| format!("tok{}", i)).collect();
        let tokens_b: Vec<String> = tokens_a.clone();
        let file_tokens = vec![
            ("a.ts".to_string(), tokens_a),
            ("b.ts".to_string(), tokens_b),
        ];
        let clusters = find_near_duplicates(&file_tokens, 1.0);
        assert!(clusters.is_empty(), "threshold 1.0 should disable near-duplicate detection");
    }
}
