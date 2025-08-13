use crate::engine::StyleEngine;
use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub fn generate_css(
    class_names: &HashSet<String>,
    output_path: &Path,
    engine: &StyleEngine,
    _file_classnames: &HashMap<PathBuf, HashSet<String>>,
) {
    let is_production = std::env::var("DX_ENV").map_or(false, |v| v == "production");

    if !is_production {
        // Collect and sort class names for deterministic output, which is crucial for development.
        let mut sorted_class_names: Vec<_> = class_names.iter().collect();
        sorted_class_names.sort_unstable(); // This is a fast, in-place sort.

        // Generate the CSS rules in parallel to speed up computation.
        // The engine's internal cache is thread-safe.
        let css_rules: Vec<String> = sorted_class_names
            .par_iter() // Use Rayon's parallel iterator
            .filter_map(|class_name| engine.generate_css_for_class(class_name))
            .collect();

        if css_rules.is_empty() {
            fs::write(output_path, "").expect("Failed to write empty CSS file");
            return;
        }
        
        // Join the results and write to the file efficiently.
        let final_css = css_rules.join("\n\n");
        let file = File::create(output_path).expect("Failed to create CSS file");
        let mut writer = BufWriter::new(file);
        writer.write_all(final_css.as_bytes()).expect("Failed to write CSS");
        writer.write_all(b"\n").expect("Failed to write final newline");
        writer.flush().expect("Failed to flush writer");
        return;
    }

    // Production path remains the same for minification.
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
