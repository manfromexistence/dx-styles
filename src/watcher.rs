use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use crate::{data_manager, generator, parser, utils};
use crate::engine::StyleEngine;
use std::time::Instant;

pub fn process_file_change(
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_file: &Path,
    engine: &StyleEngine,
) {
    let start = Instant::now();
    let new_classnames = parser::parse_classnames(path);
    let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(path, &new_classnames, file_classnames, classname_counts, global_classnames);

    if added_global > 0 || removed_global > 0 {
        generator::generate_css(global_classnames, output_file, engine);
    }
    let time_us = start.elapsed().as_micros();
    utils::log_change(path, added_file, removed_file, output_file, added_global, removed_global, time_us);
}

pub fn process_file_remove(
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_file: &Path,
    engine: &StyleEngine,
) {
    if let Some(old_classnames) = file_classnames.remove(path) {
        let start = Instant::now();
        let mut removed_in_global = 0;
        for cn in &old_classnames {
            if let Some(count) = classname_counts.get_mut(cn) {
                *count -= 1;
                if *count == 0 {
                    global_classnames.remove(cn);
                    removed_in_global += 1;
                }
            }
        }
        if removed_in_global > 0 {
            generator::generate_css(global_classnames, output_file, engine);
        }
        let time_us = start.elapsed().as_micros();
        utils::log_change(path, 0, old_classnames.len(), output_file, 0, removed_in_global, time_us);
    }
}
