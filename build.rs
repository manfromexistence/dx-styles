use std::collections::HashMap;
use std::fs;
use std::path::Path;
use flatbuffers::{FlatBufferBuilder, WIPOffset};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct TomlConfig {
    #[serde(rename = "static", default)]
    static_styles: HashMap<String, String>,
    #[serde(default)]
    dynamic: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    generators: HashMap<String, GeneratorConfig>,
}

#[derive(Deserialize, Debug, Clone)]
struct GeneratorConfig {
    multiplier: f32,
    unit: String,
}

#[derive(Debug, Clone)]
struct StyleRecord {
    name: String,
    css: String,
}

fn main() {
    let fbs_files = ["styles.fbs"];
    let toml_path = "styles.toml";
    let out_dir = std::env::var("OUT_DIR").unwrap();

    for fbs_file in fbs_files.iter() {
        println!("cargo:rerun-if-changed={}", fbs_file);
    }
    println!("cargo:rerun-if-changed={}", toml_path);

    flatc_rust::run(flatc_rust::Args {
        lang: "rust",
        inputs: &fbs_files.iter().map(|s| Path::new(s)).collect::<Vec<_>>(),
        out_dir: Path::new(&out_dir),
        includes: &[Path::new("src")],
        ..Default::default()
    })
    .expect("flatc schema compilation failed");

    let toml_content = fs::read_to_string(toml_path).expect("Failed to read styles.toml");
    let toml_data: TomlConfig = toml::from_str(&toml_content).expect("Failed to parse styles.toml");

    for (key, _values) in &toml_data.dynamic {
        let parts: Vec<&str> = key.split('|').collect();
        if parts.len() != 2 {
            panic!("Invalid dynamic key format in styles.toml: '{}'. Expected 'prefix|property'.", key);
        }
    }

    let mut precompiled_styles = Vec::new();

    for (name, css) in toml_data.static_styles {
        precompiled_styles.push(StyleRecord { name, css });
    }

    for (key, values) in toml_data.dynamic {
        let parts: Vec<&str> = key.split('|').collect();
        let prefix = parts[0];
        let property = parts[1];
        for (suffix, value) in values {
            let name = format!("{}-{}", prefix, suffix);
            let css = format!("{}: {}", property, value);
            precompiled_styles.push(StyleRecord { name, css });
        }
    }

    precompiled_styles.sort_by(|a, b| a.name.cmp(&b.name));

    let mut builder = FlatBufferBuilder::new();

    let mut style_offsets = Vec::new();
    for style in &precompiled_styles {
        let name_offset = builder.create_string(&style.name);
        let css_offset = builder.create_string(&style.css);
        
        let table_wip = builder.start_table();
        builder.push_slot(4, name_offset, WIPOffset::new(0));
        builder.push_slot(6, css_offset, WIPOffset::new(0));
        let style_offset = builder.end_table(table_wip);
        style_offsets.push(style_offset);
    }
    let styles_vec = builder.create_vector(&style_offsets);

    let mut generator_offsets = Vec::new();
    for (key, config) in toml_data.generators {
        let parts: Vec<&str> = key.split('|').collect();
        if parts.len() != 2 { continue; }
        
        let prefix_offset = builder.create_string(parts[0]);
        let property_offset = builder.create_string(parts[1]);
        let unit_offset = builder.create_string(&config.unit);

        let table_wip = builder.start_table();
        builder.push_slot(4, prefix_offset, WIPOffset::new(0));
        builder.push_slot(6, property_offset, WIPOffset::new(0));
        builder.push_slot(8, config.multiplier, 0.0f32);
        builder.push_slot(10, unit_offset, WIPOffset::new(0));
        let gen_offset = builder.end_table(table_wip);
        generator_offsets.push(gen_offset);
    }
    let generators_vec = builder.create_vector(&generator_offsets);

    let table_wip = builder.start_table();
    builder.push_slot(4, styles_vec, WIPOffset::new(0));
    builder.push_slot(6, generators_vec, WIPOffset::new(0));
    let config_root = builder.end_table(table_wip);

    builder.finish(config_root, None);

    let buf = builder.finished_data();
    let styles_bin_path = Path::new(".dx/styles.bin");
    fs::create_dir_all(styles_bin_path.parent().unwrap()).expect("Failed to create .dx directory");
    fs::write(styles_bin_path, buf).expect("Failed to write styles.bin");

    println!("✅ Successfully generated .dx/styles.bin from styles.toml");
}
