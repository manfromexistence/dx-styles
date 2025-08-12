use crate::engine::StyleEngine;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
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

    let file = File::create(output_path).expect("Failed to create output file");
    let mut writer = BufWriter::new(file);
    writer
        .write_all(css_content.as_bytes())
        .expect("Failed to write CSS");
}