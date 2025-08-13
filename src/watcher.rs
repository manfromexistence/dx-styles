use crate::{
    cache::ClassnameCache, data_manager, engine::StyleEngine, generator, parser, utils,
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Instant,
};

pub fn process_file_change(
    cache: &ClassnameCache,
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_path: &Path,
    style_engine: &StyleEngine,
) {
    let start = Instant::now();
    let classnames = parser::parse_classnames(path);
    let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(
        path,
        &classnames,
        file_classnames,
        classname_counts,
        global_classnames,
    );

    if added_global > 0 || removed_global > 0 {
        generator::generate_css(
            global_classnames,
            output_path,
            style_engine,
            file_classnames,
        );
    }

    utils::log_change(
        "✨",
        path,
        added_file,
        removed_file,
        output_path,
        added_global,
        removed_global,
        start.elapsed().as_micros(),
    );

    let _ = cache.set(path, &classnames);
}

pub fn process_file_remove(
    cache: &ClassnameCache,
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_path: &Path,
    style_engine: &StyleEngine,
) {
    let start = Instant::now();
    let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(
        path,
        &HashSet::new(),
        file_classnames,
        classname_counts,
        global_classnames,
    );

    if removed_global > 0 {
        generator::generate_css(
            global_classnames,
            output_path,
            style_engine,
            file_classnames,
        );
    }
    
    utils::log_change(
        "🗑️",
        path,
        added_file,
        removed_file,
        output_path,
        added_global,
        removed_global,
        start.elapsed().as_micros(),
    );

    let _ = cache.remove(path);
}
