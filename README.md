# Dx
Enhance Developer Experience

```bash
tree -a -I "inspirations|target|.git"
```

### Suggestions for Improving dx-styles

1. **Cache Optimization**:
   - Implement a cache eviction policy (e.g., LRU) for the in-memory cache to manage memory usage.
   - Periodically sync `cache.bin` to disk in a background thread to reduce I/O during active use.

2. **Parallel Processing**:
   - Optimize `generator.rs` to use Rayon for chunked CSS generation, reducing contention on large codebases.
   - Parallelize file scanning in `utils.rs` with Rayon iterators for faster directory traversal.

3. **Performance Monitoring**:
   - Add metrics (e.g., `prometheus` crate) to track cache hit/miss rates and serialization times.
   - Profile FlatBuffers serialization vs. bincode in benchmarks to confirm performance gains.

4. **Error Handling**:
   - Use `thiserror` for custom error types to improve error handling across modules.
   - Centralize error logging for better debugging.

5. **Maintainability**:
   - Refactor `parser.rs` into smaller modules (e.g., separate AST traversal logic).
   - Use feature flags to enable/disable optional features like debugging logs.

6. **Testing**:
   - Add unit tests for `ClassnameCache` to validate FlatBuffers serialization.
   - Create integration tests for file watching and cache consistency.

7. **Configuration**:
   - Support dynamic reloading of `styles.toml` and `cache.fbs` without restarting.
   - Cache parsed configurations in memory to avoid repeated parsing.

### Setup
```md
# Project: Tailwind to dx-styles.toml Transformation

**Task Assigned By:** manfromexistence
**Date:** August 7, 2025
**Status:** Awaiting `tailwind_classes.json` input file.

## 1. Objective

The primary goal is to process a comprehensive JSON file containing all Tailwind CSS utility classes and their corresponding CSS rules. This data must then be intelligently sorted and formatted into a new `dx-styles.toml` file, adhering to its specific structural rules for static, dynamic, and generated utilities.

This task is a core part of building the `dx-styles` engine for the **dx** project.

---

## 2. Input Assets

1.  **`tailwind_classes.json` (To be provided)**: A large JSON object where keys are the Tailwind CSS class names (e.g., `"p-4"`) and values are their complete CSS definitions (e.g., `"padding: 1rem;"`).
2.  **`dx-styles.toml` (Target Schema)**: The output file must follow this TOML structure.

### Target TOML Schema Example:

```toml
# [static]: For one-off, unchanging classes.
[static]
flex = "display: flex;"
h-full = "height: 100%;"

# [dynamic]: For classes with a common prefix and a non-numeric scale.
[dynamic]
"text|font-size" = { xs = "0.75rem", sm = "0.875rem", base = "1rem" }

# [generators]: For classes generated from any number.
[generators]
"p|padding" = { multiplier = 0.25, unit = "rem" }
```

```bash
echo "Item                                     (Size)      (Files)     (Folders)"
echo "--------------------------------------------------------------------------------"
(
    for item in */; do
        if [ -d "$item" ]; then
            size_bytes=$(du -sb "$item" 2>/dev/null | awk '{print $1}')
            size_human=$(du -sh "$item" 2>/dev/null | awk '{print $1}')
            files=$(find "$item" -type f | wc -l)
            folders=$(find "$item" -mindepth 1 -type d | wc -l)
            printf "%s %-40s | %-10s| %-10s| %-10s\n" "$size_bytes" "$item" "($size_human)" "($files)" "($folders)"
        fi
    done

    for item in *; do
        if [ -f "$item" ]; then
            size_bytes=$(stat -c %s "$item" 2>/dev/null)
            size_human=$(ls -lh "$item" 2>/dev/null | awk '{print $5}')
            printf "%s %-40s | %-10s| %-10s| %-10s\n" "$size_bytes" "$item" "($size_human)" "(0)" "(0)"
        fi
    done
) | sort -rn | cut -d' ' -f2-
```


```rust
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use colored::Colorize;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use oxc_allocator::Allocator;
use oxc_ast::ast::{self, ExportDefaultDeclarationKind, JSXAttributeItem, JSXOpeningElement, Program};
use oxc_parser::Parser;
use oxc_span::SourceType;
use walkdir::WalkDir;

mod styles_generated {
    #![allow(dead_code, unused_imports, unsafe_op_in_unsafe_fn)]
    include!(concat!(env!("OUT_DIR"), "/styles_generated.rs"));
}
use styles_generated::style_schema;

struct StyleEngine {
    precompiled: HashMap<String, String>,
    buffer: Vec<u8>,
}

impl StyleEngine {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
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

    fn generate_css_for_class(&self, class_name: &str) -> Option<String> {
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

fn main() {
    let style_engine = match StyleEngine::new() {
        Ok(engine) => engine,
        Err(e) => {
            println!("{} Failed to initialize StyleEngine: {}. Please run 'cargo build' to generate it.", "Error:".red(), e);
            return;
        }
    };
    println!("{}", "✅ Dx Styles initialized with new Style Engine.".bold().green());

    let dir = Path::new("src");
    let output_file = Path::new(".").join("styles.css");

    let mut file_classnames: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut classname_counts: HashMap<String, u32> = HashMap::new();
    let mut global_classnames: HashSet<String> = HashSet::new();
    let mut pending_events: HashMap<PathBuf, Instant> = HashMap::new();

    let scan_start = Instant::now();
    let files = find_tsx_jsx_files(dir);
    if !files.is_empty() {
        let mut total_added_in_files = 0;
        for file in &files {
            let new_classnames = parse_classnames(file);
            let (added, _, _, _) = update_maps(file, &new_classnames, &mut file_classnames, &mut classname_counts, &mut global_classnames);
            total_added_in_files += added;
        }
        generate_css(&global_classnames, &output_file, &style_engine);
        log_change(dir, total_added_in_files, 0, &output_file, global_classnames.len(), 0, scan_start.elapsed().as_micros());
    } else {
        println!("{}", "No .tsx or .jsx files found in src/.".yellow());
    }

    println!("{}", "Dx Styles is watching for file changes...".bold().cyan());

    let (tx, rx) = channel();
    let config = Config::default().with_poll_interval(Duration::from_millis(50));
    let mut watcher = RecommendedWatcher::new(tx, config).unwrap();
    watcher.watch(dir, RecursiveMode::Recursive).unwrap();

    let mut event_queue: VecDeque<(PathBuf, bool)> = VecDeque::new();

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                for path in event.paths {
                    if is_tsx_jsx(&path) {
                        let is_remove = matches!(event.kind, notify::EventKind::Remove(_));
                        event_queue.push_back((path, is_remove));
                    }
                }
            }
            Ok(Err(e)) => println!("Watch error: {:?}", e),
            Err(_) => {
                let mut processed_paths = HashSet::new();
                let now = Instant::now();
                while let Some((path, is_remove)) = event_queue.pop_front() {
                    if processed_paths.contains(&path) {
                        continue;
                    }
                    if let Some(last_time) = pending_events.get(&path) {
                        if now.duration_since(*last_time) < Duration::from_millis(100) {
                            event_queue.push_back((path.clone(), is_remove));
                            continue;
                        }
                    }
                    if is_remove {
                        process_file_remove(&path, &mut file_classnames, &mut classname_counts, &mut global_classnames, &output_file, &style_engine);
                    } else {
                        process_file_change(&path, &mut file_classnames, &mut classname_counts, &mut global_classnames, &output_file, &style_engine);
                    }
                    pending_events.insert(path.clone(), now);
                    processed_paths.insert(path);
                }
            }
        }
    }
}

fn find_tsx_jsx_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| is_tsx_jsx(e.path()))
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn is_tsx_jsx(path: &Path) -> bool {
    path.extension().map_or(false, |ext| ext == "tsx" || ext == "jsx")
}

fn parse_classnames(path: &Path) -> HashSet<String> {
    let source_text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return HashSet::new(),
    };
    if source_text.is_empty() {
        return HashSet::new();
    }

    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_default().with_jsx(true);
    let ret = Parser::new(&allocator, &source_text, source_type).parse();

    let mut visitor = ClassNameVisitor { class_names: HashSet::new() };
    visitor.visit_program(&ret.program);
    visitor.class_names
}

struct ClassNameVisitor {
    class_names: HashSet<String>,
}

impl ClassNameVisitor {
    fn visit_program(&mut self, program: &Program) {
        for stmt in &program.body {
            self.visit_statement(stmt);
        }
    }

    fn visit_statement(&mut self, stmt: &ast::Statement) {
        match stmt {
            ast::Statement::ExpressionStatement(stmt) => self.visit_expression(&stmt.expression),
            ast::Statement::BlockStatement(stmt) => {
                for s in &stmt.body {
                    self.visit_statement(s);
                }
            }
            ast::Statement::ReturnStatement(stmt) => {
                if let Some(arg) = &stmt.argument {
                    self.visit_expression(arg);
                }
            }
            ast::Statement::IfStatement(stmt) => {
                self.visit_statement(&stmt.consequent);
                if let Some(alt) = &stmt.alternate {
                    self.visit_statement(alt);
                }
            }
            ast::Statement::VariableDeclaration(decl) => {
                for var in &decl.declarations {
                    if let Some(init) = &var.init {
                        self.visit_expression(init);
                    }
                }
            }
            ast::Statement::FunctionDeclaration(decl) => self.visit_function(decl),
            ast::Statement::ExportNamedDeclaration(decl) => {
                if let Some(decl) = &decl.declaration {
                    self.visit_declaration(decl);
                }
            }
            ast::Statement::ExportDefaultDeclaration(decl) => self.visit_export_default_declaration(decl),
            _ => {}
        }
    }

    fn visit_declaration(&mut self, decl: &ast::Declaration) {
        match decl {
            ast::Declaration::FunctionDeclaration(func) => self.visit_function(func),
            ast::Declaration::VariableDeclaration(var_decl) => {
                for var in &var_decl.declarations {
                    if let Some(init) = &var.init {
                        self.visit_expression(init);
                    }
                }
            }
            _ => {}
        }
    }
    
    fn visit_export_default_declaration(&mut self, decl: &ast::ExportDefaultDeclaration) {
        match &decl.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => self.visit_function(func),
            ExportDefaultDeclarationKind::ArrowFunctionExpression(expr) => {
                for stmt in &expr.body.statements {
                    self.visit_statement(stmt);
                }
            }
            kind => {
                if let Some(expr) = kind.as_expression() {
                    self.visit_expression(expr);
                }
            }
        }
    }

    fn visit_function(&mut self, func: &ast::Function) {
        if let Some(body) = &func.body {
            for stmt in &body.statements {
                self.visit_statement(stmt);
            }
        }
    }

    fn visit_expression(&mut self, expr: &ast::Expression) {
        match expr {
            ast::Expression::JSXElement(elem) => self.visit_jsx_element(elem),
            ast::Expression::JSXFragment(frag) => self.visit_jsx_fragment(frag),
            ast::Expression::ConditionalExpression(expr) => {
                self.visit_expression(&expr.consequent);
                self.visit_expression(&expr.alternate);
            }
            ast::Expression::ArrowFunctionExpression(expr) => {
                for stmt in &expr.body.statements {
                    self.visit_statement(stmt);
                }
            }
            ast::Expression::ParenthesizedExpression(expr) => self.visit_expression(&expr.expression),
            _ => {}
        }
    }

    fn visit_jsx_element(&mut self, elem: &ast::JSXElement) {
        self.visit_jsx_opening_element(&elem.opening_element);
        for child in &elem.children {
            self.visit_jsx_child(child);
        }
    }

    fn visit_jsx_fragment(&mut self, frag: &ast::JSXFragment) {
        for child in &frag.children {
            self.visit_jsx_child(child);
        }
    }

    fn visit_jsx_child(&mut self, child: &ast::JSXChild) {
        match child {
            ast::JSXChild::Element(elem) => self.visit_jsx_element(elem),
            ast::JSXChild::Fragment(frag) => self.visit_jsx_fragment(frag),
            ast::JSXChild::ExpressionContainer(container) => {
                if let Some(expr) = container.expression.as_expression() {
                    self.visit_expression(expr);
                }
            }
            _ => {}
        }
    }

    fn visit_jsx_opening_element(&mut self, elem: &JSXOpeningElement) {
        for attr in &elem.attributes {
            if let JSXAttributeItem::Attribute(attr) = attr {
                if let ast::JSXAttributeName::Identifier(ident) = &attr.name {
                    if ident.name == "className" {
                        if let Some(ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value {
                            lit.value.split_whitespace().for_each(|cn| {
                                self.class_names.insert(cn.to_string());
                            });
                        }
                    }
                }
            }
        }
    }
}

fn update_maps(
    path: &Path,
    new_classnames: &HashSet<String>,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
) -> (usize, usize, usize, usize) {
    let old_classnames = file_classnames.get(path).cloned().unwrap_or_default();
    let added_in_file: HashSet<_> = new_classnames.difference(&old_classnames).cloned().collect();
    let removed_in_file: HashSet<_> = old_classnames.difference(new_classnames).cloned().collect();

    let mut added_in_global = 0;
    let mut removed_in_global = 0;

    for cn in &removed_in_file {
        if let Some(count) = classname_counts.get_mut(cn) {
            *count -= 1;
            if *count == 0 {
                global_classnames.remove(cn);
                removed_in_global += 1;
            }
        }
    }

    for cn in &added_in_file {
        let count = classname_counts.entry(cn.clone()).or_insert(0);
        if *count == 0 {
            global_classnames.insert(cn.clone());
            added_in_global += 1;
        }
        *count += 1;
    }

    file_classnames.insert(path.to_path_buf(), new_classnames.clone());
    (added_in_file.len(), removed_in_file.len(), added_in_global, removed_in_global)
}

fn generate_css(class_names: &HashSet<String>, output_path: &Path, engine: &StyleEngine) {
    let mut file = File::create(output_path).unwrap();
    let mut sorted_class_names: Vec<_> = class_names.iter().collect();
    sorted_class_names.sort();

    for cn in sorted_class_names {
        if let Some(css_rule) = engine.generate_css_for_class(cn) {
            writeln!(file, "{}", css_rule).unwrap();
        }
    }
}

fn process_file_change(
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_file: &Path,
    engine: &StyleEngine,
) {
    let start = Instant::now();
    let new_classnames = parse_classnames(path);
    let (added_file, removed_file, added_global, removed_global) = update_maps(path, &new_classnames, file_classnames, classname_counts, global_classnames);

    if added_global > 0 || removed_global > 0 {
        generate_css(global_classnames, output_file, engine);
    }
    let time_us = start.elapsed().as_micros();
    log_change(path, added_file, removed_file, output_file, added_global, removed_global, time_us);
}

fn process_file_remove(
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
            generate_css(global_classnames, output_file, engine);
        }
        let time_us = start.elapsed().as_micros();
        log_change(path, 0, old_classnames.len(), output_file, 0, removed_in_global, time_us);
    }
}

fn log_change(
    source_path: &Path,
    added_file: usize,
    removed_file: usize,
    output_path: &Path,
    added_global: usize,
    removed_global: usize,
    time_us: u128,
) {
    if added_file == 0 && removed_file == 0 && added_global == 0 && removed_global == 0 {
        return;
    }

    let source_str = source_path.display().to_string();
    let output_str = output_path.display().to_string();

    let file_changes = format!(
        "({}, {})",
        format!("+{}", added_file).bright_green(),
        format!("-{}", removed_file).bright_red()
    );

    let output_changes = format!(
        "({}, {})",
        format!("+{}", added_global).bright_green(),
        format!("-{}", removed_global).bright_red()
    );

    let time_str = if time_us < 1000 {
        format!("{}µs", time_us)
    } else {
        format!("{}ms", time_us / 1000)
    };

    println!(
        "{} {} -> {} {} · {}",
        source_str.bright_cyan(),
        file_changes,
        output_str.bright_magenta(),
        output_changes,
        time_str.yellow()
    );
}
```
