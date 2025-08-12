use crate::engine::StyleEngine;
use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub fn generate_css(
    class_names: &HashSet<String>,
    output_path: &Path,
    engine: &StyleEngine,
    _file_classnames: &HashMap<PathBuf, HashSet<String>>,
) {
    let css_rules: Vec<_> = class_names
        .par_iter()
        .filter_map(|cn| engine.generate_css_for_class(cn))
        .collect();

    let css_content = css_rules.join("\n\n");

    // If there are no classes, ensure the file is empty.
    if css_content.is_empty() {
        let _ = fs::write(output_path, "");
        return;
    }

    let stylesheet =
        StyleSheet::parse(&css_content, ParserOptions::default()).expect("Failed to parse CSS");

    let is_production = std::env::var("DX_ENV").map(|v| v == "production").unwrap_or(false);
    let minified_css = stylesheet
        .to_css(PrinterOptions {
            minify: is_production,
            ..PrinterOptions::default()
        })
        .expect("Failed to minify CSS")
        .code;

    fs::write(output_path, minified_css).expect("Failed to write to output file");
}