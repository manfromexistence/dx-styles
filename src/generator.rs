use crate::engine::StyleEngine;
use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
use memmap2::MmapMut;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

pub fn generate_css(
    class_names: &HashSet<String>,
    output_path: &Path,
    engine: &StyleEngine,
    _file_classnames: &HashMap<PathBuf, HashSet<String>>,
) {
    let is_production = std::env::var("DX_ENV").map_or(false, |v| v == "production");

    if !is_production {
        let mut sorted_class_names: Vec<_> = class_names.iter().collect();
        sorted_class_names.sort_unstable();

        let css_rules: Vec<String> = sorted_class_names
            .par_iter()
            .filter_map(|class_name| engine.generate_css_for_class(class_name))
            .collect();

        if css_rules.is_empty() {
            fs::write(output_path, "").expect("Failed to write empty CSS file");
            return;
        }

        let total_size = css_rules.iter().map(|s| s.len()).sum::<usize>()
            + (css_rules.len().saturating_sub(1)) * 2
            + 1;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(output_path)
            .expect("Failed to open output file for memory-mapping");

        file.set_len(total_size as u64)
            .expect("Failed to set file length");

        let mut mmap = unsafe { MmapMut::map_mut(&file).expect("Failed to memory-map output file") };

        let mut cursor = 0;
        for (i, rule) in css_rules.iter().enumerate() {
            let rule_bytes = rule.as_bytes();
            let end = cursor + rule_bytes.len();
            mmap[cursor..end].copy_from_slice(rule_bytes);
            cursor = end;

            if i < css_rules.len() - 1 {
                mmap[cursor..cursor + 2].copy_from_slice(b"\n\n");
                cursor += 2;
            }
        }
        mmap[cursor..cursor + 1].copy_from_slice(b"\n");

        return;
    }

    let css_rules: Vec<String> = class_names
        .par_iter()
        .filter_map(|cn| engine.generate_css_for_class(cn))
        .collect();

    if css_rules.is_empty() {
        let _ = fs::write(output_path, b"");
        return;
    }

    let css_content = css_rules.join("\n");

    let stylesheet =
        StyleSheet::parse(&css_content, ParserOptions::default()).expect("Failed to parse CSS");

    let minified_css = stylesheet
        .to_css(PrinterOptions {
            minify: true,
            ..Default::default()
        })
        .expect("Failed to minify CSS")
        .code;

    fs::write(output_path, minified_css).expect("Failed to write minified CSS");
}
