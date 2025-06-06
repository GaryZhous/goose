use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use include_dir::{include_dir, Dir};
use minijinja::{Environment, Error as MiniJinjaError, Value as MJValue};
use once_cell::sync::Lazy;
use serde::Serialize;

/// This directory will be embedded into the final binary.
/// Typically used to store "core" or "system" prompts.
static CORE_PROMPTS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/prompts");

/// A global MiniJinja environment storing the "core" prompts.
///
/// - Loaded at startup from the `CORE_PROMPTS_DIR`.
/// - Ideal for "system" templates that don't change often.
/// - *Not* used for extension prompts (which are ephemeral).
static GLOBAL_ENV: Lazy<Arc<RwLock<Environment<'static>>>> = Lazy::new(|| {
    let mut env = Environment::new();

    // Pre-load all core templates from the embedded dir.
    for file in CORE_PROMPTS_DIR.files() {
        let name = file.path().to_string_lossy().to_string();
        let source = String::from_utf8_lossy(file.contents()).to_string();

        // Since we're using 'static lifetime for the Environment, we need to ensure
        // the strings we add as templates live for the entire program duration.
        // We can achieve this by leaking the strings (acceptable for initialization).
        let static_name: &'static str = Box::leak(name.into_boxed_str());
        let static_source: &'static str = Box::leak(source.into_boxed_str());

        if let Err(e) = env.add_template(static_name, static_source) {
            println!("Failed to add template {}: {}", static_name, e);
        }
    }

    Arc::new(RwLock::new(env))
});

/// Renders a prompt from the global environment by name.
///
/// # Arguments
/// * `template_name` - The name of the template (usually the file path or a custom ID).
/// * `context_data`  - Data to be inserted into the template (must be `Serialize`).
pub fn render_global_template<T: Serialize>(
    template_name: &str,
    context_data: &T,
) -> Result<String, MiniJinjaError> {
    let env = GLOBAL_ENV.read().expect("GLOBAL_ENV lock poisoned");
    let tmpl = env.get_template(template_name)?;
    let ctx = MJValue::from_serialize(context_data);
    let rendered = tmpl.render(ctx)?;
    Ok(rendered.trim().to_string())
}

/// Renders a file from `CORE_PROMPTS_DIR` within the global environment.
///
/// # Arguments
/// * `template_file` - The file path within the embedded directory (e.g. "system.md").
/// * `context_data`  - Data to be inserted into the template (must be `Serialize`).
///
/// This function **assumes** the file is already in `CORE_PROMPTS_DIR`. If it wasn't
/// added to the global environment at startup (due to parse errors, etc.), this will error out.
pub fn render_global_file<T: Serialize>(
    template_file: impl Into<PathBuf>,
    context_data: &T,
) -> Result<String, MiniJinjaError> {
    let file_path = template_file.into();
    let template_name = file_path.to_string_lossy().to_string();

    render_global_template(&template_name, context_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// For convenience in tests, define a small struct or use a HashMap to provide context.
    #[derive(Serialize)]
    struct TestContext {
        name: String,
        age: u32,
    }

    #[test]
    fn test_global_file_render() {
        // "mock.md" should exist in the embedded CORE_PROMPTS_DIR
        // and have placeholders for `name` and `age`.
        let context = TestContext {
            name: "Alice".to_string(),
            age: 30,
        };

        let result = render_global_file("mock.md", &context).unwrap();
        // Assume mock.md content is something like:
        //  "This prompt is only used for testing.\n\nHello, {{ name }}! You are {{ age }} years old."
        assert_eq!(
            result,
            "This prompt is only used for testing.\n\nHello, Alice! You are 30 years old."
        );
    }

    #[test]
    fn test_global_file_not_found() {
        let context = TestContext {
            name: "Unused".to_string(),
            age: 99,
        };

        let result = render_global_file("non_existent.md", &context);
        assert!(result.is_err(), "Should fail because file is missing");
    }
}
