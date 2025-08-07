use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use crate::engine::StyleEngine;

pub fn generate_css(class_names: &HashSet<String>, output_path: &Path, engine: &StyleEngine) {
    let mut file = File::create(output_path).unwrap();
    let mut sorted_class_names: Vec<_> = class_names.iter().collect();
    sorted_class_names.sort();

    for cn in sorted_class_names {
        if let Some(css_rule) = engine.generate_css_for_class(cn) {
            writeln!(file, "{}", css_rule).unwrap();
        }
    }
}
