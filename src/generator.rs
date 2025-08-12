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
        let mut sorted_class_names: Vec<_> = class_names.iter().collect();
        sorted_class_names.sort_unstable();

        let file = File::create(output_path).expect("Failed to create output file");
        let mut writer = BufWriter::with_capacity(8192, file);

        for class_name in sorted_class_names {
            if let Some(css_rule) = engine.generate_css_for_class(class_name) {
                writeln!(writer, "{}\n", css_rule).expect("Failed to write to buffer");
            }
        }
        writer.flush().expect("Failed to flush buffer to file");
        return;
    }

    let css_rules: Vec<String> = class_names
        .par_iter()
        .filter_map(|cn| engine.generate_css_for_class(cn))
        .collect();

    let css_content = css_rules.join("\n");

    if css_content.is_empty() {
        let _ = fs::write(output_path, b"");
        return;
    }

    let stylesheet =
        StyleSheet::parse(&css_content, ParserOptions::default()).expect("Failed to parse CSS");

    let minified_css = stylesheet
        .to_css(PrinterOptions {
            minify: true,
            ..PrinterOptions::default()
        })
        .expect("Failed to minify CSS")
        .code;

    fs::write(output_path, minified_css).expect("Failed to write to output file");
}