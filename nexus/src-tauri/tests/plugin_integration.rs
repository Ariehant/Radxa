//! Integration test: plugin discovery and loading in a real vault.

#[test]
fn plugin_loader_discovers_valid_plugin() {
    use nexus_lib::plugin::loader::PluginLoader;
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path();

    // Create plugin directory structure.
    let plugins_dir = vault_root.join(".nexus").join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();

    // Create a valid plugin.
    let plugin_dir = plugins_dir.join("com.example.test");
    fs::create_dir(&plugin_dir).unwrap();

    let manifest_text = r#"
[plugin]
id = "com.example.test"
name = "Test Plugin"
version = "1.0.0"
min_nexus_version = "0.1.0"

[entry]
ui = "index.js"
"#;
    fs::write(plugin_dir.join("plugin.toml"), manifest_text).unwrap();

    // Load plugins.
    let loader = PluginLoader::new(&vault_root.to_path_buf());
    let plugins = loader.load_all().unwrap();

    // Verify discovery.
    assert_eq!(plugins.len(), 1);
    assert!(plugins.contains_key("com.example.test"));
    let (manifest, loaded_dir) = plugins.get("com.example.test").unwrap();
    assert_eq!(manifest.plugin.name, "Test Plugin");
    assert_eq!(loaded_dir, &plugin_dir);
}

#[test]
fn plugin_manifest_validates_on_load() {
    use nexus_lib::plugin::loader::PluginLoader;
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let vault_root = temp_dir.path();

    let plugins_dir = vault_root.join(".nexus").join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();

    // Create an invalid plugin (bad id format).
    let bad_plugin_dir = plugins_dir.join("BadId");
    fs::create_dir(&bad_plugin_dir).unwrap();

    let bad_manifest = r#"
[plugin]
id = "BadId"
name = "Bad Plugin"
version = "1.0.0"
min_nexus_version = "0.1.0"

[entry]
ui = "index.js"
"#;
    fs::write(bad_plugin_dir.join("plugin.toml"), bad_manifest).unwrap();

    // Load should skip the invalid plugin gracefully.
    let loader = PluginLoader::new(&vault_root.to_path_buf());
    let plugins = loader.load_all().unwrap();

    assert!(plugins.is_empty(), "invalid plugin was skipped");
}
