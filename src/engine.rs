use std::collections::HashMap;
use std::fs;

mod styles_generated {
    #![allow(dead_code, unused_imports, unsafe_op_in_unsafe_fn)]
    include!(concat!(env!("OUT_DIR"), "/styles_generated.rs"));
}
use styles_generated::style_schema;

pub struct StyleEngine {
    precompiled: HashMap<String, String>,
    buffer: Vec<u8>,
}

impl StyleEngine {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let buffer = fs::read("styles.bin")?;
        let config = unsafe { flatbuffers::root_unchecked::<style_schema::Config>(&buffer) };

        let mut precompiled = HashMap::new();
        if let Some(styles) = config.styles() {
            for style in styles {
                let name = style.name();
                if let Some(css) = style.css() {
                    precompiled.insert(name.to_string(), css.to_string());
                }
            }
        }

        Ok(Self {
            precompiled,
            buffer,
        })
    }

    pub fn generate_css_for_class(&self, class_name: &str) -> Option<String> {
        if let Some(css) = self.precompiled.get(class_name) {
            return Some(format!(".{} {{\n    {}\n}}", class_name, css));
        }

        let config = unsafe { flatbuffers::root_unchecked::<style_schema::Config>(&self.buffer) };
        if let Some(generators) = config.generators() {
            for generator in generators {
                if let (Some(prefix), Some(property), Some(unit)) = (
                    generator.prefix(),
                    generator.property(),
                    generator.unit(),
                ) {
                    if class_name.starts_with(&format!("{}-", prefix)) {
                        let value_str = &class_name[prefix.len() + 1..];
                        if let Ok(num_val) = value_str.parse::<f32>() {
                            let final_value = num_val * generator.multiplier();
                            let css = format!("{}: {}{};", property, final_value, unit);
                            return Some(format!(".{} {{\n    {}\n}}", class_name, css));
                        }
                    }
                }
            }
        }
        None
    }
}
