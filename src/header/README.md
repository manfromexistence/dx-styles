```
// mod header;
// mod platform;
// pub use crate::platform::{dimensions, dimensions_stderr, dimensions_stdin, dimensions_stdout};

// fn main() {
//     println!("--- Default Render (Left Aligned) ---");
//     header::render("Hello, how are you?");

//     println!("\n--- Customized Render (Centered) ---");
//     let font = header::DXCliFont::default().expect("Failed to load the default font.");

//     if let Some(figure) = font.figure("DX-CLI") {
//         let centered_figure = figure.align(header::Alignment::Center);
//         println!("{}", centered_figure);
//     }

//     println!("\n--- Customized Render (Right Aligned) ---");
//     if let Some(figure) = font.figure("Rust Forge") {
//         let right_aligned_figure = figure.align(header::Alignment::Right);
//         println!("{}", right_aligned_figure);
//     }
// }
```
