use std::collections::{HashMap, HashSet};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use crate::panels::launchfiles::{LaunchArgument, LaunchFile, LaunchKind, LaunchPackage};
#[cfg(not(target_arch = "wasm32"))]
use crate::panels::parameters::workspace::{find_packages_in_workspace, should_ignore_dir};

/// Escaneia toda a workspace ROS em busca de pacotes e seus arquivos de launch
#[cfg(not(target_arch = "wasm32"))]
pub fn scan_workspace_launchfiles<P: AsRef<Path>>(workspace_root: P) -> Vec<LaunchPackage> {
    let root = workspace_root.as_ref();
    if !root.exists() {
        return Vec::new();
    }

    let found_pkgs = find_packages_in_workspace(root);
    let mut raw_packages = Vec::new();
    // Mapeamento de (pkg_name, launch_file_rel_path) -> Vec<(target_pkg, target_rel_path)>
    let mut includes_map: HashMap<(String, String), Vec<(String, String)>> = HashMap::new();

    for (pkg_name, pkg_dir) in found_pkgs {
        let launch_files = find_launch_files_in_package(&pkg_name, &pkg_dir, &mut includes_map);
        if !launch_files.is_empty() {
            raw_packages.push(LaunchPackage {
                name: pkg_name,
                launch_files,
            });
        }
    }

    build_launch_hierarchy(raw_packages, &includes_map)
}

/// Encontra todos os arquivos de launch dentro das pastas `launch/` de um pacote
#[cfg(not(target_arch = "wasm32"))]
pub fn find_launch_files_in_package(
    pkg_name: &str,
    pkg_dir: &Path,
    includes_map: &mut HashMap<(String, String), Vec<(String, String)>>,
) -> Vec<LaunchFile> {
    let mut launch_dirs = Vec::new();
    find_launch_dirs(pkg_dir, pkg_dir, &mut launch_dirs);

    let mut files = Vec::new();
    for l_dir in launch_dirs {
        collect_launch_files_in_dir(pkg_name, &l_dir, &l_dir, &mut files, includes_map);
    }

    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

#[cfg(not(target_arch = "wasm32"))]
fn find_launch_dirs(pkg_dir: &Path, current_dir: &Path, launch_dirs: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(current_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if path.is_dir() {
                if path != pkg_dir && path.join("package.xml").exists() {
                    continue;
                }
                if should_ignore_dir(name) {
                    continue;
                }
                if name == "launch" {
                    launch_dirs.push(path);
                } else {
                    find_launch_dirs(pkg_dir, &path, launch_dirs);
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn collect_launch_files_in_dir(
    pkg_name: &str,
    launch_root: &Path,
    current_dir: &Path,
    files: &mut Vec<LaunchFile>,
    includes_map: &mut HashMap<(String, String), Vec<(String, String)>>,
) {
    let Ok(entries) = std::fs::read_dir(current_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if path.is_dir() {
                if !should_ignore_dir(name) {
                    collect_launch_files_in_dir(pkg_name, launch_root, &path, files, includes_map);
                }
            } else if path.is_file() && is_launch_file_name(name) {
                let rel_path = path
                    .strip_prefix(launch_root)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| name.to_string());

                let kind = if name.ends_with(".py") {
                    LaunchKind::Python
                } else {
                    LaunchKind::Xml
                };

                let content = std::fs::read_to_string(&path).unwrap_or_default();
                let desc = extract_launch_description(&content, kind);

                let args = if kind == LaunchKind::Xml {
                    extract_xml_args(&content)
                } else {
                    Vec::new()
                };

                if kind == LaunchKind::Xml {
                    let incs = extract_xml_includes(&content);
                    if !incs.is_empty() {
                        includes_map.insert((pkg_name.to_string(), rel_path.clone()), incs);
                    }
                }

                files.push(LaunchFile::new(rel_path, kind, desc).with_arguments(args));
            }
        }
    }
}

/// Verifica se um arquivo segue os critérios de launchfile
pub fn is_launch_file_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower.ends_with(".py") {
        return true;
    }
    lower.contains("launch") || lower.contains("launchfile")
}

/// Extrai a descrição de um launch file com base no seu tipo
pub fn extract_launch_description(content: &str, kind: LaunchKind) -> String {
    match kind {
        LaunchKind::Xml => extract_xml_description(content),
        LaunchKind::Python => extract_python_description(content),
    }
}

/// Extrai a descrição em comentários `<!-- ... -->` acima de `<launch>`
pub fn extract_xml_description(content: &str) -> String {
    let pre_launch = if let Some(idx) = content.find("<launch") {
        &content[..idx]
    } else {
        content
    };

    let mut comments = Vec::new();
    let mut cursor = pre_launch;

    while let Some(start) = cursor.find("<!--") {
        let rest = &cursor[start + 4..];
        if let Some(end) = rest.find("-->") {
            let comment_text = rest[..end].trim();
            // Ignora declarações xml <?xml ...?> ou comentários vazios
            if !comment_text.is_empty() && !comment_text.starts_with('?') {
                comments.push(comment_text.to_string());
            }
            cursor = &rest[end + 3..];
        } else {
            break;
        }
    }

    comments.join(" ")
}

/// Extrai a descrição em comentários `#` ou strings `'''...'''` / `"""..."""` no topo
pub fn extract_python_description(content: &str) -> String {
    let mut cursor = content.trim_start();
    if cursor.is_empty() {
        return String::new();
    }

    // Pula shebang se presente no topo (ex: #!/usr/bin/env python3)
    if cursor.starts_with("#!") {
        if let Some(newline_idx) = cursor.find('\n') {
            cursor = cursor[newline_idx + 1..].trim_start();
        }
    }

    // 1. Verifica se começa com docstring multilinhas """ ou '''
    for delimiter in &["\"\"\"", "'''"] {
        if cursor.starts_with(delimiter) {
            let after = &cursor[delimiter.len()..];
            if let Some(end_idx) = after.find(delimiter) {
                let doc = after[..end_idx].trim();
                let clean_doc = doc
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                return clean_doc;
            }
        }
    }

    // 2. Extrai bloco de comentários iniciados por '#'
    let mut comment_lines = Vec::new();
    for line in cursor.lines() {
        let l = line.trim();
        if l.starts_with('#') {
            let text = l.trim_start_matches('#').trim();
            if !text.is_empty() {
                // Se for cabeçalho longo de licença genérica, pode acumular ou filtrar
                comment_lines.push(text);
            }
        } else if l.is_empty() {
            if !comment_lines.is_empty() {
                break;
            }
        } else {
            // Chegou no código python executável
            break;
        }
    }

    // Se as linhas contiverem licença típica da autoware/f1tenth/ros e não houver descrição útil,
    // verifica se tem alguma linha que descreve o propósito do arquivo
    let joined = comment_lines.join(" ");
    if joined.starts_with("Copyright ") || joined.starts_with("Licensed under ") {
        // Se for puramente licença, retornamos vazio para não poluir
        return String::new();
    }

    joined
}

/// Extrai tags `<include file="$(find-pkg-share <pkg>)/launch/<rel_path>" />`
pub fn extract_xml_includes(content: &str) -> Vec<(String, String)> {
    let mut includes = Vec::new();
    let mut cursor = content;

    while let Some(inc_idx) = cursor.find("<include") {
        let rest = &cursor[inc_idx..];
        let tag_end = rest.find('>').unwrap_or(rest.len());
        let tag_content = &rest[..tag_end];

        if let Some(share_idx) = tag_content.find("$(find-pkg-share ") {
            let after_share = &tag_content[share_idx + 17..];
            if let Some(close_paren) = after_share.find(')') {
                let pkg_name = after_share[..close_paren].trim().to_string();
                let after_close = &after_share[close_paren + 1..];

                // Espera /launch/<path> ou /<path>
                if let Some(quote_idx) = after_close.find('"') {
                    let path_part = &after_close[..quote_idx];
                    let clean_path = path_part
                        .trim_start_matches('/')
                        .trim_start_matches("launch/")
                        .trim_start_matches('/');
                    if !clean_path.is_empty() {
                        includes.push((pkg_name, clean_path.to_string()));
                    }
                }
            }
        }

        cursor = &rest[tag_end..];
    }

    includes
}

/// Extrai tags `<arg name="..." default="..." />` de arquivos XML
pub fn extract_xml_args(content: &str) -> Vec<LaunchArgument> {
    let mut args = Vec::new();
    let mut cursor = content;

    while let Some(arg_idx) = cursor.find("<arg") {
        let rest = &cursor[arg_idx..];
        let tag_end = rest.find('>').unwrap_or(rest.len());
        let tag_content = &rest[..tag_end];

        // Extrai atributo name
        let name_opt = if let Some(n_idx) = tag_content.find("name=\"") {
            let after = &tag_content[n_idx + 6..];
            after.find('"').map(|end| after[..end].trim().to_string())
        } else if let Some(n_idx) = tag_content.find("name='") {
            let after = &tag_content[n_idx + 6..];
            after.find('\'').map(|end| after[..end].trim().to_string())
        } else {
            None
        };

        if let Some(name) = name_opt {
            if !name.is_empty() {
                // Extrai default se presente (ou value se usado como default)
                let default_val = if let Some(d_idx) = tag_content.find("default=\"") {
                    let after = &tag_content[d_idx + 9..];
                    after.find('"').map(|end| after[..end].trim().to_string()).unwrap_or_default()
                } else if let Some(d_idx) = tag_content.find("default='") {
                    let after = &tag_content[d_idx + 9..];
                    after.find('\'').map(|end| after[..end].trim().to_string()).unwrap_or_default()
                } else if let Some(v_idx) = tag_content.find("value=\"") {
                    let after = &tag_content[v_idx + 7..];
                    after.find('"').map(|end| after[..end].trim().to_string()).unwrap_or_default()
                } else if let Some(v_idx) = tag_content.find("value='") {
                    let after = &tag_content[v_idx + 7..];
                    after.find('\'').map(|end| after[..end].trim().to_string()).unwrap_or_default()
                } else {
                    String::new()
                };

                // Deduplica por nome: mantém a primeira ocorrência (ou atualiza se a existente estiver sem valor default)
                if let Some(existing) = args.iter_mut().find(|a: &&mut LaunchArgument| a.name == name) {
                    if existing.default_value.is_empty() && !default_val.is_empty() {
                        existing.default_value = default_val;
                    }
                } else {
                    args.push(LaunchArgument::new_from_file(name, default_val));
                }
            }
        }

        cursor = &rest[tag_end..];
    }

    args
}

/// Organiza os launch files em hierarquia e remove os incluídos da lista raiz para evitar duplicação
pub fn build_launch_hierarchy(
    packages: Vec<LaunchPackage>,
    includes_map: &HashMap<(String, String), Vec<(String, String)>>,
) -> Vec<LaunchPackage> {
    // 1. Dicionário rápido de (pkg_name, filename) -> LaunchFile
    let mut all_files: HashMap<(String, String), LaunchFile> = HashMap::new();
    for pkg in &packages {
        for f in &pkg.launch_files {
            all_files.insert((pkg.name.clone(), f.name.clone()), f.clone());
        }
    }

    // 2. Coletar todos os arquivos que são alvos de include em qualquer lugar
    let mut included_targets: HashSet<(String, String)> = HashSet::new();
    for targets in includes_map.values() {
        for target in targets {
            included_targets.insert(target.clone());
        }
    }

    // 3. Montar recursivamente os filhos
    fn resolve_children(
        parent_key: &(String, String),
        all_files: &HashMap<(String, String), LaunchFile>,
        includes_map: &HashMap<(String, String), Vec<(String, String)>>,
        visited: &mut HashSet<(String, String)>,
    ) -> Vec<LaunchFile> {
        let mut children = Vec::new();
        let Some(targets) = includes_map.get(parent_key) else {
            return children;
        };

        for target in targets {
            if visited.contains(target) {
                // Previne ciclos recursivos
                continue;
            }
            visited.insert(target.clone());

            let mut child_file = if let Some(existing) = all_files.get(target) {
                existing.clone()
            } else {
                let kind = if target.1.ends_with(".py") {
                    LaunchKind::Python
                } else {
                    LaunchKind::Xml
                };
                LaunchFile::new(&target.1, kind, "")
            };

            child_file.children = resolve_children(target, all_files, includes_map, visited);
            visited.remove(target);

            children.push(child_file);
        }

        children
    }

    // 4. Montar os pacotes finais apenas com arquivos que não foram incluídos como sub-launchfiles
    let mut result_packages = Vec::new();

    for pkg in packages {
        let mut top_level_files = Vec::new();

        for f in pkg.launch_files {
            let key = (pkg.name.clone(), f.name.clone());
            // Se o arquivo é filho de outro, não entra na raiz do pacote
            if included_targets.contains(&key) {
                continue;
            }

            let mut file_with_children = f;
            let mut visited = HashSet::new();
            visited.insert(key.clone());
            file_with_children.children =
                resolve_children(&key, &all_files, includes_map, &mut visited);

            top_level_files.push(file_with_children);
        }

        if !top_level_files.is_empty() {
            result_packages.push(LaunchPackage {
                name: pkg.name,
                launch_files: top_level_files,
            });
        }
    }

    result_packages.sort_by(|a, b| a.name.cmp(&b.name));
    result_packages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_launch_file_name() {
        assert!(is_launch_file_name("perse.launch"));
        assert!(is_launch_file_name("sim_launch.py"));
        assert!(is_launch_file_name("test_node.launch.xml"));
        assert!(is_launch_file_name("custom_launchfile.txt"));
        assert!(!is_launch_file_name("CMakeLists.txt"));
        assert!(!is_launch_file_name("package.xml"));
        assert!(!is_launch_file_name("params.yaml"));
    }

    #[test]
    fn test_xml_description_extraction() {
        let xml = r#"<!-- Roda o node do bumper para colisões -->
<launch>
  <node pkg="perse_drivers" exec="bumper_node" name="bumper_node" />
</launch>"#;
        assert_eq!(
            extract_xml_description(xml),
            "Roda o node do bumper para colisões"
        );

        let xml_multi = r#"<?xml version="1.0"?>
<!-- Descrição do sistema principal -->
<launch>
</launch>"#;
        assert_eq!(
            extract_xml_description(xml_multi),
            "Descrição do sistema principal"
        );
    }

    #[test]
    fn test_python_description_extraction() {
        let py_doc = r#"#!/usr/bin/env python3
"""Inicia a simulação do robô no Webots com controladores."""

import os
from launch import LaunchDescription
"#;
        assert_eq!(
            extract_python_description(py_doc),
            "Inicia a simulação do robô no Webots com controladores."
        );

        let py_comment = r#"# Driver para o sensor inercial LSM9DS1 via I2C
# Publica dados brutos e calibrados
import os
"#;
        assert_eq!(
            extract_python_description(py_comment),
            "Driver para o sensor inercial LSM9DS1 via I2C Publica dados brutos e calibrados"
        );
    }

    #[test]
    fn test_xml_includes_extraction() {
        let xml = r#"<launch>
    <include file="$(find-pkg-share perse_bringup)/launch/planner.launch" />
    <include file="$(find-pkg-share perse_webots)/launch/sim_launch.py" />
    <include file="$(find-pkg-share perse_drivers)/launch/encoders/encoders.launch" />
</launch>"#;

        let incs = extract_xml_includes(xml);
        assert_eq!(incs.len(), 3);
        assert_eq!(
            incs[0],
            ("perse_bringup".to_string(), "planner.launch".to_string())
        );
        assert_eq!(
            incs[1],
            ("perse_webots".to_string(), "sim_launch.py".to_string())
        );
        assert_eq!(
            incs[2],
            (
                "perse_drivers".to_string(),
                "encoders/encoders.launch".to_string()
            )
        );
    }

    #[test]
    fn test_launch_hierarchy_deduplication() {
        let packages = vec![
            LaunchPackage {
                name: "perse_bringup".into(),
                launch_files: vec![
                    LaunchFile::new("perse.launch", LaunchKind::Xml, "Master launch"),
                    LaunchFile::new("planner.launch", LaunchKind::Xml, "Planner launch"),
                ],
            },
            LaunchPackage {
                name: "perse_webots".into(),
                launch_files: vec![LaunchFile::new(
                    "sim_launch.py",
                    LaunchKind::Python,
                    "Simulation",
                )],
            },
        ];

        let mut includes_map = HashMap::new();
        includes_map.insert(
            ("perse_bringup".to_string(), "perse.launch".to_string()),
            vec![
                ("perse_bringup".to_string(), "planner.launch".to_string()),
                ("perse_webots".to_string(), "sim_launch.py".to_string()),
            ],
        );

        let resolved = build_launch_hierarchy(packages, &includes_map);

        // perse_bringup deve conter apenas perse.launch na raiz
        let bringup = resolved.iter().find(|p| p.name == "perse_bringup").unwrap();
        assert_eq!(bringup.launch_files.len(), 1);
        let master = &bringup.launch_files[0];
        assert_eq!(master.name, "perse.launch");

        // Os filhos de perse.launch devem ser planner.launch e sim_launch.py
        assert_eq!(master.children.len(), 2);
        assert_eq!(master.children[0].name, "planner.launch");
        assert_eq!(master.children[1].name, "sim_launch.py");

        // perse_webots não deve ter mais sim_launch.py na raiz (o pacote todo é omitido pois não tem mais arquivos raiz)
        assert!(resolved.iter().all(|p| p.name != "perse_webots"));
    }

    #[test]
    fn test_scan_real_perse_workspace_launchfiles() {
        let ws_path = Path::new("/home/alan/Documents/repos/perse");
        if ws_path.exists() {
            let pkgs = scan_workspace_launchfiles(ws_path);
            assert!(
                !pkgs.is_empty(),
                "Deve encontrar pacotes com launchfiles na workspace real"
            );

            let bringup = pkgs.iter().find(|p| p.name == "perse_bringup");
            assert!(bringup.is_some(), "perse_bringup deve ser encontrado");

            let b_pkg = bringup.unwrap();
            let perse_launch = b_pkg.launch_files.iter().find(|f| f.name == "perse.launch");
            assert!(perse_launch.is_some(), "perse.launch deve ser encontrado");

            let p_file = perse_launch.unwrap();
            assert!(
                !p_file.children.is_empty(),
                "perse.launch deve conter sub-launchfiles incluídos"
            );

            // planner.launch e localization.launch foram incluídos em perse.launch, portanto não devem estar na raiz de perse_bringup
            assert!(
                !b_pkg.launch_files.iter().any(|f| f.name == "planner.launch"),
                "planner.launch não deve estar duplicado na raiz do pacote"
            );
        }
    }

    #[test]
    fn test_extract_xml_args() {
        let xml = r#"<launch>
    <arg name="sim" default="false" description="Modo simulador"/>
    <arg name="speed" default="1.5"/>
    <arg name="robot_name" default="perse"/>
    <!-- Comentário com <arg name="ignore" default="true"/> -->
</launch>"#;
        let args = extract_xml_args(xml);
        assert_eq!(args.len(), 4); // extrai as tags arg
        assert_eq!(args[0].name, "sim");
        assert_eq!(args[0].default_value, "false");
        assert_eq!(args[0].value, "");
        assert!(args[0].from_file);

        assert_eq!(args[1].name, "speed");
        assert_eq!(args[1].default_value, "1.5");
        assert_eq!(args[1].value, "");
        assert!(args[1].from_file);

        assert_eq!(args[2].name, "robot_name");
        assert_eq!(args[2].default_value, "perse");
    }
}
