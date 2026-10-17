use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::panels::parameters::{PackageParams, ParamFile, Parameter, ParameterValue, Scope};

/// Diretórios que devem ser ignorados ao escanear a workspace
pub const IGNORED_DIRS: &[&str] = &[
    "build",
    "install",
    "log",
    ".git",
    "target",
    "node_modules",
    ".cargo",
    "deps/install",
];

/// Verifica se um diretório deve ser ignorado na busca
pub fn should_ignore_dir(dir_name: &str) -> bool {
    dir_name.starts_with('.') || IGNORED_DIRS.contains(&dir_name)
}

/// Tenta localizar dinamicamente a raiz da workspace ROS subindo a partir de um diretório
#[cfg(not(target_arch = "wasm32"))]
pub fn find_workspace_root<P: AsRef<Path>>(start_dir: P) -> Option<PathBuf> {
    let mut current = std::fs::canonicalize(start_dir.as_ref())
        .ok()
        .unwrap_or_else(|| start_dir.as_ref().to_path_buf());
    loop {
        // Se a pasta atual tem 'src' e 'build'/'install', ou contém pacotes diretamente
        if current.join("src").is_dir()
            || (current.join("install").is_dir() && current.join("build").is_dir())
        {
            return Some(current);
        }
        if let Ok(entries) = std::fs::read_dir(&current) {
            let mut count = 0;
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && p.join("package.xml").is_file() {
                    count += 1;
                }
            }
            if count > 0 {
                return Some(current);
            }
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Encontra todos os pacotes ROS na workspace (pastas contendo package.xml)
#[cfg(not(target_arch = "wasm32"))]
pub fn find_packages_in_workspace<P: AsRef<Path>>(workspace_root: P) -> Vec<(String, PathBuf)> {
    let mut packages = Vec::new();
    let root = workspace_root.as_ref();
    if !root.is_dir() {
        return packages;
    }

    scan_dir_for_packages(root, &mut packages);
    packages.sort_by(|a, b| a.0.cmp(&b.0));
    packages
}

#[cfg(not(target_arch = "wasm32"))]
fn scan_dir_for_packages(dir: &Path, packages: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let mut has_package_xml = false;
    let mut subdirs = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if path.is_file() && name == "package.xml" {
                has_package_xml = true;
            } else if path.is_dir() && !should_ignore_dir(name) {
                subdirs.push(path);
            }
        }
    }

    if has_package_xml {
        let pkg_name = extract_package_name(dir);
        packages.push((pkg_name, dir.to_path_buf()));
    }

    for sub in subdirs {
        scan_dir_for_packages(&sub, packages);
    }
}

/// Extrai o nome do pacote a partir da tag `<name>` do package.xml
#[cfg(not(target_arch = "wasm32"))]
fn extract_package_name(pkg_dir: &Path) -> String {
    let pkg_xml = pkg_dir.join("package.xml");
    if let Ok(content) = std::fs::read_to_string(&pkg_xml) {
        if let Some(start) = content.find("<name>") {
            let after = &content[start + 6..];
            if let Some(end) = after.find("</name>") {
                let name = after[..end].trim();
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
    }
    pkg_dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

/// Encontra e processa todos os arquivos de parâmetros .yaml em um pacote
#[cfg(not(target_arch = "wasm32"))]
pub fn find_param_files_in_package(pkg_dir: &Path) -> Vec<ParamFile> {
    let mut yaml_files = Vec::new();
    collect_yaml_files(pkg_dir, pkg_dir, &mut yaml_files);
    yaml_files.sort_by(|a, b| a.filename.cmp(&b.filename));
    yaml_files
}

#[cfg(not(target_arch = "wasm32"))]
fn collect_yaml_files(pkg_dir: &Path, current_dir: &Path, files: &mut Vec<ParamFile>) {
    let Ok(entries) = std::fs::read_dir(current_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if path.is_dir() {
                // Se for outro pacote aninhado, não analisa dentro deste pacote pai
                if path != pkg_dir && path.join("package.xml").exists() {
                    continue;
                }
                if !should_ignore_dir(name) {
                    collect_yaml_files(pkg_dir, &path, files);
                }
            } else if path.is_file() && (name.ends_with(".yaml") || name.ends_with(".yml")) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Arquivo de parâmetro ROS 2 válido DEVE conter 'ros__parameters'
                    if content.contains("ros__parameters") {
                        let rel_filename = path
                            .strip_prefix(pkg_dir)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| name.to_string());
                        let abs_path = path.to_string_lossy().to_string();

                        if let Some(param_file) =
                            parse_yaml_param_file(&content, &rel_filename, &abs_path)
                        {
                            files.push(param_file);
                        }
                    }
                }
            }
        }
    }
}

/// Escaneia toda a workspace e retorna os pacotes com seus arquivos de parâmetros válidos
#[cfg(not(target_arch = "wasm32"))]
pub fn scan_workspace_packages<P: AsRef<Path>>(workspace_root: P) -> Vec<PackageParams> {
    let mut packages = Vec::new();
    let root = workspace_root.as_ref();
    if !root.exists() {
        return packages;
    }

    let found_pkgs = find_packages_in_workspace(root);
    for (pkg_name, pkg_dir) in found_pkgs {
        let files = find_param_files_in_package(&pkg_dir);
        if !files.is_empty() {
            packages.push(PackageParams {
                name: pkg_name,
                files,
            });
        }
    }

    packages.sort_by(|a, b| a.name.cmp(&b.name));
    packages
}

/// Analisa o conteúdo YAML de um arquivo e gera a estrutura de escopos e parâmetros
pub fn parse_yaml_param_file(content: &str, filename: &str, file_path: &str) -> Option<ParamFile> {
    let comments_map = extract_comments_map(content);
    let Ok(yaml_val) = serde_yaml::from_str::<serde_yaml::Value>(content) else {
        return None;
    };

    let mut scopes = Vec::new();

    if let serde_yaml::Value::Mapping(root_map) = yaml_val {
        for (node_key, node_val) in root_map {
            let Some(node_name) = node_key.as_str() else {
                continue;
            };

            if let serde_yaml::Value::Mapping(ref inner_map) = node_val {
                if let Some(params_val) =
                    inner_map.get(&serde_yaml::Value::String("ros__parameters".into()))
                {
                    let mut current_path = vec![node_name.to_string()];
                    let scope =
                        parse_yaml_scope(node_name, params_val, &comments_map, &mut current_path);
                    scopes.push(scope);
                } else {
                    let mut current_path = vec![node_name.to_string()];
                    let scope =
                        parse_yaml_scope(node_name, &node_val, &comments_map, &mut current_path);
                    scopes.push(scope);
                }
            }
        }
    }

    Some(ParamFile {
        filename: filename.to_string(),
        path: file_path.to_string(),
        scopes,
    })
}

/// Converte um valor YAML em um escopo ou subescopo recursivamente
fn parse_yaml_scope(
    scope_name: &str,
    value: &serde_yaml::Value,
    comments_map: &HashMap<Vec<String>, String>,
    current_path: &mut Vec<String>,
) -> Scope {
    let mut scope = Scope::new(scope_name);

    if let serde_yaml::Value::Mapping(map) = value {
        for (k, v) in map {
            if let Some(key_str) = k.as_str() {
                current_path.push(key_str.to_string());

                if let serde_yaml::Value::Mapping(_) = v {
                    let sub_scope = parse_yaml_scope(key_str, v, comments_map, current_path);
                    scope.sub_scopes.push(sub_scope);
                } else if let Some(param_val) = yaml_value_to_param_value(v) {
                    let description = comments_map.get(current_path).cloned().unwrap_or_default();
                    let param = Parameter::new(key_str, description, param_val);
                    scope.parameters.push(param);
                }

                current_path.pop();
            }
        }
    }

    scope
}

/// Converte `serde_yaml::Value` para `ParameterValue`
pub fn yaml_value_to_param_value(v: &serde_yaml::Value) -> Option<ParameterValue> {
    match v {
        serde_yaml::Value::Bool(b) => Some(ParameterValue::Bool(*b)),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(ParameterValue::Int(i))
            } else if let Some(f) = n.as_f64() {
                Some(ParameterValue::Float(f))
            } else {
                None
            }
        }
        serde_yaml::Value::String(s) => Some(ParameterValue::String(s.clone())),
        serde_yaml::Value::Sequence(seq) => {
            if seq.is_empty() {
                return Some(ParameterValue::StringArray(Vec::new()));
            }
            if seq.iter().all(|x| x.is_bool()) {
                let arr: Vec<bool> = seq.iter().filter_map(|x| x.as_bool()).collect();
                Some(ParameterValue::BoolArray(arr))
            } else if seq.iter().all(|x| x.is_i64()) {
                let arr: Vec<i64> = seq.iter().filter_map(|x| x.as_i64()).collect();
                Some(ParameterValue::IntArray(arr))
            } else if seq.iter().all(|x| x.is_number()) {
                let arr: Vec<f64> = seq.iter().filter_map(|x| x.as_f64()).collect();
                Some(ParameterValue::FloatArray(arr))
            } else {
                let arr: Vec<String> = seq
                    .iter()
                    .map(|x| match x {
                        serde_yaml::Value::String(s) => s.clone(),
                        _ => format!("{:?}", x),
                    })
                    .collect();
                Some(ParameterValue::StringArray(arr))
            }
        }
        _ => None,
    }
}

/// Extrai mapeamento de comentários a partir do texto original do arquivo YAML
pub fn extract_comments_map(content: &str) -> HashMap<Vec<String>, String> {
    let mut map = HashMap::new();
    let mut current_comments: Vec<String> = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new(); // (indent_level, key_name)

    for line in content.lines() {
        let trimmed_end = line.trim_end();
        let trimmed = trimmed_end.trim_start();

        if trimmed.is_empty() {
            current_comments.clear();
            continue;
        }

        // Se for linha de comentário: # ...
        if trimmed.starts_with('#') {
            let comment_text = trimmed.trim_start_matches('#').trim().to_string();
            if !comment_text.is_empty() {
                current_comments.push(comment_text);
            }
            continue;
        }

        let indent = line.len() - trimmed.len();

        if let Some(colon_pos) = find_colon_key_pos(trimmed) {
            let key = trimmed[..colon_pos]
                .trim()
                .trim_matches('\'')
                .trim_matches('"');
            let remainder = trimmed[colon_pos + 1..].trim();

            while let Some((last_indent, _)) = stack.last() {
                if *last_indent >= indent {
                    stack.pop();
                } else {
                    break;
                }
            }

            let mut full_path: Vec<String> = stack.iter().map(|(_, k)| k.clone()).collect();
            full_path.push(key.to_string());

            let inline_comment = if let Some(hash_pos) = remainder.find('#') {
                remainder[hash_pos + 1..].trim().to_string()
            } else {
                String::new()
            };

            let combined_desc = if !current_comments.is_empty() {
                current_comments.join("\n")
            } else {
                inline_comment
            };

            if !combined_desc.is_empty() {
                map.insert(full_path.clone(), combined_desc.clone());

                let path_no_ros: Vec<String> = full_path
                    .iter()
                    .filter(|k| *k != "ros__parameters")
                    .cloned()
                    .collect();
                map.insert(path_no_ros, combined_desc);
            }

            current_comments.clear();

            let val_before_comment = remainder.split('#').next().unwrap_or("").trim();
            if val_before_comment.is_empty() {
                stack.push((indent, key.to_string()));
            }
        }
    }

    map
}

/// Encontra a posição do ':' separador de chave, desconsiderando aspas
fn find_colon_key_pos(line: &str) -> Option<usize> {
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for (i, c) in line.char_indices() {
        match c {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ':' if !in_single_quote && !in_double_quote => return Some(i),
            _ => {}
        }
    }
    None
}

/// Converte um ParameterValue em string YAML adequada
pub fn format_param_yaml_value(val: &ParameterValue) -> String {
    match val {
        ParameterValue::Bool(b) => format!("{}", b),
        ParameterValue::Int(i) => format!("{}", i),
        ParameterValue::Float(f) => {
            if f.fract() == 0.0 {
                format!("{:.1}", f)
            } else {
                format!("{}", f)
            }
        }
        ParameterValue::String(s) => {
            if s.contains(' ')
                || s.contains('#')
                || s.contains(':')
                || s.is_empty()
                || s == "true"
                || s == "false"
            {
                format!("\"{}\"", s)
            } else {
                s.clone()
            }
        }
        ParameterValue::BoolArray(arr) => {
            let inner = arr
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", inner)
        }
        ParameterValue::IntArray(arr) => {
            let inner = arr
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", inner)
        }
        ParameterValue::FloatArray(arr) => {
            let inner = arr
                .iter()
                .map(|f| {
                    if f.fract() == 0.0 {
                        format!("{:.1}", f)
                    } else {
                        format!("{}", f)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", inner)
        }
        ParameterValue::StringArray(arr) => {
            let inner = arr
                .iter()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", inner)
        }
        ParameterValue::ByteArray(arr) => {
            let inner = arr
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", inner)
        }
    }
}

/// Coleta recursivamente todos os parâmetros com seu caminho hierárquico
fn collect_params_recursive(
    scope: &Scope,
    prefix_path: &[String],
    out: &mut HashMap<Vec<String>, ParameterValue>,
) {
    let mut current_path = prefix_path.to_vec();
    current_path.push(scope.name.clone());

    for param in &scope.parameters {
        let mut param_path = current_path.clone();
        param_path.push(param.name.clone());
        out.insert(param_path, param.edited_value.clone());
    }

    for sub in &scope.sub_scopes {
        collect_params_recursive(sub, &current_path, out);
    }
}

/// Aplica as alterações de parâmetros sobre o conteúdo de texto do arquivo YAML
pub fn apply_param_updates_to_yaml(
    original_yaml: &str,
    updates: &HashMap<Vec<String>, ParameterValue>,
) -> String {
    let mut new_lines: Vec<String> = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();

    for line in original_yaml.lines() {
        let trimmed_end = line.trim_end();
        let trimmed = trimmed_end.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            new_lines.push(line.to_string());
            continue;
        }

        let indent = line.len() - trimmed.len();

        if let Some(colon_pos) = find_colon_key_pos(trimmed) {
            let key = trimmed[..colon_pos]
                .trim()
                .trim_matches('\'')
                .trim_matches('"');
            let remainder = trimmed[colon_pos + 1..].trim();

            while let Some((last_indent, _)) = stack.last() {
                if *last_indent >= indent {
                    stack.pop();
                } else {
                    break;
                }
            }

            let mut full_path_no_ros: Vec<String> = stack
                .iter()
                .filter(|(_, k)| k != "ros__parameters")
                .map(|(_, k)| k.clone())
                .collect();
            full_path_no_ros.push(key.to_string());

            let updated_line = if let Some(new_val) = updates.get(&full_path_no_ros) {
                let leading_ws = &line[..indent];
                let key_prefix = format!("{}{}: ", leading_ws, key);
                let val_str = format_param_yaml_value(new_val);

                let inline_comment = if let Some(hash_pos) = remainder.find('#') {
                    format!(" {}", &remainder[hash_pos..])
                } else {
                    String::new()
                };

                format!("{}{}{}", key_prefix, val_str, inline_comment)
            } else {
                line.to_string()
            };

            new_lines.push(updated_line);

            let val_before_comment = remainder.split('#').next().unwrap_or("").trim();
            if val_before_comment.is_empty() {
                stack.push((indent, key.to_string()));
            }
        } else {
            new_lines.push(line.to_string());
        }
    }

    let mut result = new_lines.join("\n");
    if original_yaml.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Salva um ParamFile de volta no disco, preservando formatação e comentários
#[cfg(not(target_arch = "wasm32"))]
pub fn save_param_file(file: &mut ParamFile) -> Result<(), std::io::Error> {
    if file.path.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Caminho do arquivo não especificado",
        ));
    }

    let original = std::fs::read_to_string(&file.path)?;
    let mut updates = HashMap::new();

    for scope in &file.scopes {
        collect_params_recursive(scope, &[], &mut updates);
    }

    let updated_yaml = apply_param_updates_to_yaml(&original, &updates);
    std::fs::write(&file.path, updated_yaml)?;

    file.apply_all();
    re_log::info!("Arquivo de parâmetros salvo com sucesso: {}", file.path);
    Ok(())
}

/// Salva todos os arquivos modificados nos pacotes fornecidos
#[cfg(not(target_arch = "wasm32"))]
pub fn save_modified_packages(packages: &mut [PackageParams]) -> Result<usize, std::io::Error> {
    let mut saved_count = 0;
    for pkg in packages.iter_mut() {
        for file in &mut pkg.files {
            if file.count_modified() > 0 {
                save_param_file(file)?;
                saved_count += 1;
            }
        }
    }
    Ok(saved_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_comment_extraction_and_parsing() {
        let yaml_text = r#"
motors_node:
    ros__parameters:
        # limits the command sent to vesc_driver
        # positive angles are 0 -> 0.5
        max_angle_command: 0.5 # radians

        servo_offset: 0.0
"#;
        let param_file =
            parse_yaml_param_file(yaml_text, "test.yaml", "/path/test.yaml").unwrap();
        assert_eq!(param_file.scopes.len(), 1);
        let node_scope = &param_file.scopes[0];
        assert_eq!(node_scope.name, "motors_node");
        assert_eq!(node_scope.parameters.len(), 2);

        let p1 = &node_scope.parameters[0];
        assert_eq!(p1.name, "max_angle_command");
        assert_eq!(p1.saved_value, ParameterValue::Float(0.5));
        assert!(p1
            .description
            .contains("limits the command sent to vesc_driver"));
        assert!(p1.description.contains("positive angles are 0 -> 0.5"));

        let p2 = &node_scope.parameters[1];
        assert_eq!(p2.name, "servo_offset");
        assert_eq!(p2.saved_value, ParameterValue::Float(0.0));
    }

    #[test]
    fn test_apply_param_updates_preserves_comments_and_format() {
        let yaml_text = r#"
/**:
    ros__parameters:
        properties:
            wheel:
                # Wheel radius in meters
                wheel_radius: 0.05755 # meters
                gear_ratio: 9.6
"#;
        let mut updates = HashMap::new();
        updates.insert(
            vec![
                "/**".into(),
                "properties".into(),
                "wheel".into(),
                "wheel_radius".into(),
            ],
            ParameterValue::Float(0.065),
        );

        let updated = apply_param_updates_to_yaml(yaml_text, &updates);
        assert!(updated.contains("wheel_radius: 0.065 # meters"));
        assert!(updated.contains("# Wheel radius in meters"));
        assert!(updated.contains("gear_ratio: 9.6"));
    }

    #[test]
    fn test_scan_real_perse_workspace() {
        let ws = "/home/alan/Documents/repos/perse";
        if std::path::Path::new(ws).exists() {
            let packages = scan_workspace_packages(ws);
            assert!(!packages.is_empty(), "Deveria encontrar pacotes na workspace do Perse");

            let has_bringup = packages.iter().any(|p| p.name == "perse_bringup");
            let has_drivers = packages.iter().any(|p| p.name == "perse_drivers");
            assert!(has_bringup, "perse_bringup deve estar presente");
            assert!(has_drivers, "perse_drivers deve estar presente");

            let bringup = packages.iter().find(|p| p.name == "perse_bringup").unwrap();
            let properties_file = bringup
                .files
                .iter()
                .find(|f| f.filename.contains("perse_properties.yaml"));
            assert!(
                properties_file.is_some(),
                "perse_properties.yaml deve ser encontrado"
            );
            let p_file = properties_file.unwrap();
            assert!(!p_file.scopes.is_empty());
        }
    }

    #[test]
    fn test_save_param_file_roundtrip() {
        let temp_dir = std::env::temp_dir().join("perse_test_param_save");
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("test_params.yaml");

        let initial_yaml = r#"
my_node:
    ros__parameters:
        # Distance between wheels in meters
        wheel_track: 0.5 # meters
        active: false
"#;
        std::fs::write(&test_file, initial_yaml).unwrap();

        let mut param_file = parse_yaml_param_file(
            initial_yaml,
            "test_params.yaml",
            test_file.to_str().unwrap(),
        )
        .unwrap();

        let param = &mut param_file.scopes[0].parameters[0];
        assert_eq!(param.name, "wheel_track");
        param.edited_value = ParameterValue::Float(0.65);
        assert!(param.is_modified());

        save_param_file(&mut param_file).unwrap();
        assert!(!param_file.scopes[0].parameters[0].is_modified());

        let disk_content = std::fs::read_to_string(&test_file).unwrap();
        assert!(disk_content.contains("wheel_track: 0.65 # meters"));
        assert!(disk_content.contains("# Distance between wheels in meters"));
        assert!(disk_content.contains("active: false"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
