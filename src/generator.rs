use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use lightningcss::stylesheet::{ParserOptions, StyleSheet, PrinterOptions};
use rayon::prelude::*;
use crate::engine::StyleEngine;

pub fn generate_css(
    class_names: &HashSet<String>,
    output_path: &Path,
    engine: &StyleEngine,
    _file_classnames: &HashMap<PathBuf, HashSet<String>>,
) {
    let existing_css: HashMap<String, String> = match fs::read_to_string(output_path) {
        Ok(content) => content
            .split('}')
            .filter(|s| !s.trim().is_empty())
            .map(|rule| {
                let rule = rule.trim();
                let class_name = rule[1..rule.find('{').unwrap()].to_string();
                (class_name, rule.to_string() + "}")
            })
            .collect(),
        Err(_) => HashMap::new(),
    };

    let css_rules: Vec<_> = class_names.par_iter()
        .filter_map(|cn| {
            if let Some(existing) = existing_css.get(cn) {
                if let Some(new_css) = engine.generate_css_for_class(cn) {
                    if new_css == *existing {
                        return Some(existing.to_string());
                    }
                }
            }
            engine.generate_css_for_class(cn)
        })
        .collect();

    let css_content = css_rules.join("\n");
    let stylesheet = StyleSheet::parse(&css_content, ParserOptions::default()).expect("Failed to parse CSS");

    let is_production = std::env::var("DX_ENV").map(|v| v == "production").unwrap_or(false);
    let minified_css = stylesheet.to_css(PrinterOptions {
        minify: is_production,
        ..PrinterOptions::default()
    }).expect("Failed to minify CSS").code;

    let mut file = File::create(output_path).expect("Failed to create output file");
    file.write_all(minified_css.as_bytes()).expect("Failed to write CSS");
}