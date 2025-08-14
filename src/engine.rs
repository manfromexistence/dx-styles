use lru::LruCache;
use std::collections::HashMap;
use std::fs;
use std::num::NonZeroUsize;
use std::sync::Mutex;

mod styles_generated {
    #![allow(
        dead_code,
        unused_imports,
        unsafe_op_in_unsafe_fn,
        mismatched_lifetime_syntaxes
    )]
    include!(concat!(env!("OUT_DIR"), "/styles_generated.rs"));
}
use styles_generated::style_schema;

pub struct StyleEngine {
    precompiled: HashMap<String, String>,
    buffer: Vec<u8>,
    screens: HashMap<String, String>,
    states: HashMap<String, String>,
    container_queries: HashMap<String, String>,
    css_cache: Mutex<LruCache<String, String>>,
}

impl StyleEngine {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let buffer = fs::read(".dx/styles.bin")?;
        let config = flatbuffers::root::<style_schema::Config>(&buffer)
            .map_err(|e| format!("Failed to parse styles.bin: {}", e))?;

        let mut precompiled = HashMap::new();
        if let Some(styles) = config.styles() {
            for style in styles {
                let name = style.name();
                let css = style.css();
                if !name.is_empty() && !css.is_empty() {
                    precompiled.insert(name.to_string(), css.to_string());
                }
            }
        }

        if let Some(dynamics) = config.dynamics() {
            for dynamic in dynamics {
                if let Some(values) = dynamic.values() {
                    for value in values {
                        let key = dynamic.key();
                        let suffix = value.suffix();
                        let property = dynamic.property();
                        let value_str = value.value();

                        let name = if suffix.is_empty() {
                            key.to_string()
                        } else {
                            format!("{}-{}", key, suffix)
                        };
                        if name.is_empty() {
                            continue;
                        }
                        let css = format!("{}: {}", property, value_str);
                        precompiled.insert(name, css);
                    }
                }
            }
        }

        let screens = config.screens().map_or_else(HashMap::new, |s| {
            s.iter()
                .filter_map(|screen| {
                    let name = screen.name();
                    let value = screen.value();
                    if !name.is_empty() && !value.is_empty() {
                        Some((name.to_string(), value.to_string()))
                    } else {
                        None
                    }
                })
                .collect()
        });

        let states = config.states().map_or_else(HashMap::new, |s| {
            s.iter()
                .filter_map(|state| {
                    let name = state.name();
                    let value = state.value();
                    if !name.is_empty() && !value.is_empty() {
                        Some((name.to_string(), value.to_string()))
                    } else {
                        None
                    }
                })
                .collect()
        });

        let container_queries = config.container_queries().map_or_else(HashMap::new, |c| {
            c.iter()
                .filter_map(|cq| {
                    let name = cq.name();
                    let value = cq.value();
                    if !name.is_empty() && !value.is_empty() {
                        Some((name.to_string(), value.to_string()))
                    } else {
                        None
                    }
                })
                .collect()
        });

        Ok(Self {
            precompiled,
            buffer,
            screens,
            states,
            container_queries,
            css_cache: Mutex::new(LruCache::new(NonZeroUsize::new(1000).unwrap())),
        })
    }

    pub fn generate_css_for_class(&self, class_name: &str) -> Option<String> {
        if let Some(cached) = self.css_cache.lock().unwrap().get(class_name) {
            return Some(cached.clone());
        }

        let parts: Vec<&str> = class_name.split(':').collect();
        let base_class = *parts.last()?;
        let prefixes = &parts[..parts.len() - 1];

        let mut media_queries = Vec::new();
        let mut pseudo_classes = String::new();

        for prefix in prefixes {
            if let Some(screen_value) = self.screens.get(*prefix) {
                media_queries.push(format!("@media (min-width: {})", screen_value));
            } else if let Some(cq_value) = self.container_queries.get(*prefix) {
                media_queries.push(format!("@container (min-width: {})", cq_value));
            } else if let Some(state_value) = self.states.get(*prefix) {
                pseudo_classes.push_str(state_value);
            }
        }

        let core_css = self
            .precompiled
            .get(base_class)
            .cloned()
            .or_else(|| self.generate_dynamic_css(base_class));

        if let Some(css) = core_css {
            let selector = format!(".{}{}", class_name.replace(":", "\\:"), pseudo_classes);
            let css_body = format!("{} {{\n  {}\n}}", selector, css);
            let final_css = media_queries
                .iter()
                .rfold(css_body, |acc, mq| format!("{} {{\n  {}\n}}", mq, acc));
            self.css_cache
                .lock()
                .unwrap()
                .put(class_name.to_string(), final_css.clone());
            return Some(final_css);
        }

        None
    }

    fn generate_dynamic_css(&self, class_name: &str) -> Option<String> {
        let config = flatbuffers::root::<style_schema::Config>(&self.buffer).ok()?;
        if let Some(generators) = config.generators() {
            for generator in generators {
                let prefix = generator.prefix();
                let property = generator.property();
                let unit = generator.unit();

                if class_name.starts_with(&format!("{}-", prefix)) {
                    let value_str = &class_name[prefix.len() + 1..];
                    let (value_str, is_negative) = if let Some(stripped) = value_str.strip_prefix('-') {
                        (stripped, true)
                    } else {
                        (value_str, false)
                    };

                    let num_val: f32 = if value_str.is_empty() {
                        1.0
                    } else if let Ok(num) = value_str.parse::<f32>() {
                        num
                    } else {
                        continue;
                    };

                    let final_value = num_val * generator.multiplier() * if is_negative { -1.0 } else { 1.0 };
                    let css_value = if unit.is_empty() {
                        format!("{}", final_value)
                    } else {
                        format!("{}{}", final_value, unit)
                    };
                    let core_css = Some(format!("{}: {}", property, css_value));
                    return core_css;
                }
            }
        }

        None
    }
}
