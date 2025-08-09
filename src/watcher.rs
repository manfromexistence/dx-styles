use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use crate::{data_manager, engine::StyleEngine, generator, parser, utils};

pub fn process_file_change(
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_path: &Path,
    engine: &StyleEngine,
) {
    let start = Instant::now();
    let cache = crate::cache::ClassnameCache::new(".dx", "inspirations/website/app/globals.css");
    let new_classnames = cache.compare_and_generate(path).expect("Failed to compare and generate classnames");
    cache.update_from_classnames(path, &new_classnames).expect("Failed to update cache");
    let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(
        path,
        &new_classnames,
        file_classnames,
        classname_counts,
        global_classnames,
    );
    generator::generate_css(global_classnames, output_path, engine, file_classnames);
    utils::log_change(
        path,
        added_file,
        removed_file,
        output_path,
        added_global,
        removed_global,
        start.elapsed().as_micros(),
    );
}

pub fn process_file_remove(
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_path: &Path,
    engine: &StyleEngine,
) {
    let start = Instant::now();
    let cache = crate::cache::ClassnameCache::new(".dx", "inspirations/website/app/globals.css");
    let empty_classnames = HashSet::new();
    cache.update_from_classnames(path, &empty_classnames).expect("Failed to update cache");
    let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(
        path,
        &empty_classnames,
        file_classnames,
        classname_counts,
        global_classnames,
    );
    generator::generate_css(global_classnames, output_path, engine, file_classnames);
    utils::log_change(
        path,
        added_file,
        removed_file,
        output_path,
        added_global,
        removed_global,
        start.elapsed().as_micros(),
    );
}