use crate::{cache::ClassnameCache, data_manager, engine::StyleEngine, generator, parser, utils};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Instant,
};

pub fn process_file_change(
    _cache: &ClassnameCache,
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_path: &Path,
    engine: &StyleEngine,
) {
    let start = Instant::now();
    let current_classnames = parser::parse_classnames(path);

    let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(
        path,
        &current_classnames,
        file_classnames,
        classname_counts,
        global_classnames,
    );

    if added_global > 0 || removed_global > 0 {
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
}

pub fn process_file_remove(
    cache: &ClassnameCache,
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_path: &Path,
    engine: &StyleEngine,
) {
    if !file_classnames.contains_key(path) {
        return;
    }

    let start = Instant::now();
    let empty_classnames = HashSet::new();

    let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(
        path,
        &empty_classnames,
        file_classnames,
        classname_counts,
        global_classnames,
    );

    cache.remove(path).ok();

    if removed_global > 0 {
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
}