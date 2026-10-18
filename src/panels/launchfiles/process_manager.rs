use std::path::Path;
use std::time::Duration;

use crate::config::LaunchTargetMode;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunningLaunch {
    pub pid: u32,
    pub package: String,
    pub filename: String,
}

/// Extrai (pacote, arquivo_launch) a partir de uma linha de comando de processo
pub fn parse_launch_command(cmdline: &str) -> Option<(String, String)> {
    let tokens: Vec<&str> = cmdline.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    // Procura o token "launch"
    let launch_idx = tokens.iter().position(|&t| t == "launch")?;

    // Garante que antes de "launch" exista uma referência a "ros2"
    let is_ros2 = tokens[..launch_idx]
        .iter()
        .any(|t| *t == "ros2" || t.ends_with("/ros2") || t.ends_with("ros2"));

    if !is_ros2 {
        return None;
    }

    // Após "launch", coletar tokens posicionais que não sejam flags (-...) ou atribuição de args (:=)
    let mut positional = Vec::new();
    for &token in &tokens[(launch_idx + 1)..] {
        if token.starts_with('-') {
            continue;
        }
        if token.contains(":=") {
            break;
        }
        positional.push(token);
    }

    if positional.is_empty() {
        return None;
    }

    // Caso 1: ros2 launch <pacote> <arquivo>
    if positional.len() >= 2 {
        let pkg = positional[0].to_string();
        let file = Path::new(positional[1])
            .file_name()?
            .to_string_lossy()
            .to_string();
        return Some((pkg, file));
    }

    // Caso 2: ros2 launch <caminho_direto_para_arquivo>
    if positional.len() == 1 {
        let p = Path::new(positional[0]);
        let filename = p.file_name()?.to_string_lossy().to_string();
        if filename.ends_with(".py") || filename.ends_with(".xml") || filename.ends_with(".launch")
        {
            return Some((String::new(), filename));
        }
    }

    None
}

/// Executa a varredura síncrona dos launchfiles em execução
pub fn scan_running_launches(mode: LaunchTargetMode, container_name: &str) -> Vec<RunningLaunch> {
    let output = match mode {
        LaunchTargetMode::Host => std::process::Command::new("ps")
            .args(["-eo", "pid,args"])
            .output(),
        LaunchTargetMode::Docker => std::process::Command::new("docker")
            .args(["exec", container_name, "ps", "-eo", "pid,args"])
            .output(),
    };

    let Ok(out) = output else {
        return Vec::new();
    };

    if !out.status.success() {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(&out.stdout);
    parse_ps_output(&text)
}

/// Executa a varredura assíncrona dos launchfiles em execução com timeout seguro
pub async fn scan_running_launches_async(
    mode: LaunchTargetMode,
    container_name: &str,
) -> Vec<RunningLaunch> {
    let cmd_future = async {
        match mode {
            LaunchTargetMode::Host => {
                tokio::process::Command::new("ps")
                    .args(["-eo", "pid,args"])
                    .output()
                    .await
            }
            LaunchTargetMode::Docker => {
                tokio::process::Command::new("docker")
                    .args(["exec", container_name, "ps", "-eo", "pid,args"])
                    .output()
                    .await
            }
        }
    };

    let Ok(Ok(out)) = tokio::time::timeout(Duration::from_millis(1500), cmd_future).await else {
        return Vec::new();
    };

    if !out.status.success() {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(&out.stdout);
    parse_ps_output(&text)
}

fn parse_ps_output(ps_output: &str) -> Vec<RunningLaunch> {
    let mut results = Vec::new();

    for line in ps_output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("PID") {
            continue;
        }

        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let Some(pid_str) = parts.next() else {
            continue;
        };
        let Some(cmd) = parts.next() else { continue };

        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };

        if let Some((package, filename)) = parse_launch_command(cmd) {
            results.push(RunningLaunch {
                pid,
                package,
                filename,
            });
        }
    }

    results
}

/// Inicia um launchfile de forma independente no host ou no container
pub async fn start_launch_async(
    mode: LaunchTargetMode,
    container_name: &str,
    entrypoint: &str,
    package: &str,
    filename: &str,
    launch_args: &[String],
) -> Result<Option<u32>, String> {
    let entry_script = if !entrypoint.trim().is_empty() {
        format!(
            "if [ -f \"{0}\" ]; then source \"{0}\" 2>/dev/null || \"{0}\"; fi; ",
            entrypoint.trim()
        )
    } else {
        String::new()
    };

    let args_suffix = if launch_args.is_empty() {
        String::new()
    } else {
        format!(" {}", launch_args.join(" "))
    };

    match mode {
        LaunchTargetMode::Host => {
            let cmd_str = format!(
                "for s in /opt/ros/*/setup.bash install/setup.bash ~/perse_ws/install/setup.bash; do [ -f \"$s\" ] && source \"$s\" 2>/dev/null; done; {0}exec ros2 launch {1} {2}{3}",
                entry_script, package, filename, args_suffix
            );

            let child = tokio::process::Command::new("bash")
                .args(["-c", &cmd_str])
                .spawn()
                .map_err(|e| format!("Falha ao iniciar processo no host: {}", e))?;

            Ok(child.id())
        }
        LaunchTargetMode::Docker => {
            let cmd_str = format!(
                "for s in /perse_entrypoint.sh /opt/ros/*/setup.bash /home/*/*/install/setup.bash ~/perse_ws/install/setup.bash; do [ -f \"$s\" ] && source \"$s\" 2>/dev/null; done; {0}exec ros2 launch {1} {2}{3}",
                entry_script, package, filename, args_suffix
            );

            let out = tokio::process::Command::new("docker")
                .args(["exec", "-d", container_name, "bash", "-c", &cmd_str])
                .output()
                .await
                .map_err(|e| format!("Falha ao executar docker exec: {}", e))?;

            if !out.status.success() {
                let err_msg = String::from_utf8_lossy(&out.stderr);
                return Err(format!("docker exec falhou: {}", err_msg.trim()));
            }

            // Aguarda um instante para tentar capturar o PID detectado
            tokio::time::sleep(Duration::from_millis(200)).await;
            let running = scan_running_launches_async(mode, container_name).await;
            let detected = running.into_iter().find(|rl| {
                rl.package == package
                    && (rl.filename == filename
                        || rl.filename.trim_end_matches(".xml")
                            == filename.trim_end_matches(".xml"))
            });

            Ok(detected.map(|r| r.pid))
        }
    }
}

/// Para todos os processos associados a um determinado launchfile
pub async fn stop_launch_async(
    mode: LaunchTargetMode,
    container_name: &str,
    package: &str,
    filename: &str,
) -> Result<usize, String> {
    let running = scan_running_launches_async(mode, container_name).await;
    let target_pids: Vec<u32> = running
        .into_iter()
        .filter(|rl| {
            rl.package == package
                && (rl.filename == filename
                    || rl.filename.trim_end_matches(".xml") == filename.trim_end_matches(".xml"))
        })
        .map(|rl| rl.pid)
        .collect();

    if target_pids.is_empty() {
        return Ok(0);
    }

    let pids_count = target_pids.len();

    for pid in &target_pids {
        let pid_val = *pid;
        match mode {
            LaunchTargetMode::Host => {
                let _ = tokio::process::Command::new("kill")
                    .args(["-2", &pid_val.to_string()])
                    .status()
                    .await;
            }
            LaunchTargetMode::Docker => {
                let _ = tokio::process::Command::new("docker")
                    .args(["exec", container_name, "kill", "-2", &pid_val.to_string()])
                    .status()
                    .await;
            }
        }
    }

    // Tarefa em segundo plano para envio de SIGKILL caso o processo não finalize após timeout
    let c_name = container_name.to_string();
    let pids_to_check = target_pids.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(2500)).await;
        for pid in pids_to_check {
            let is_alive = match mode {
                LaunchTargetMode::Host => tokio::process::Command::new("kill")
                    .args(["-0", &pid.to_string()])
                    .status()
                    .await
                    .map(|s| s.success())
                    .unwrap_or(false),
                LaunchTargetMode::Docker => tokio::process::Command::new("docker")
                    .args(["exec", &c_name, "kill", "-0", &pid.to_string()])
                    .status()
                    .await
                    .map(|s| s.success())
                    .unwrap_or(false),
            };

            if is_alive {
                match mode {
                    LaunchTargetMode::Host => {
                        let _ = tokio::process::Command::new("kill")
                            .args(["-9", &pid.to_string()])
                            .status()
                            .await;
                    }
                    LaunchTargetMode::Docker => {
                        let _ = tokio::process::Command::new("docker")
                            .args(["exec", &c_name, "kill", "-9", &pid.to_string()])
                            .status()
                            .await;
                    }
                }
            }
        }
    });

    Ok(pids_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_launch_command_variants() {
        // Formato simples direto
        let cmd1 = "ros2 launch perse_bringup robot_bringup.launch.py";
        assert_eq!(
            parse_launch_command(cmd1),
            Some((
                "perse_bringup".to_string(),
                "robot_bringup.launch.py".to_string()
            ))
        );

        // Executado com python3 e caminho completo para o ros2
        let cmd2 =
            "/usr/bin/python3 /opt/ros/humble/bin/ros2 launch perse_bringup perse.launch.xml";
        assert_eq!(
            parse_launch_command(cmd2),
            Some(("perse_bringup".to_string(), "perse.launch.xml".to_string()))
        );

        // Com flags intermediárias antes dos posicionais
        let cmd3 = "python3 /opt/ros/humble/bin/ros2 launch -s -d perse_bringup calibrate.launch";
        assert_eq!(
            parse_launch_command(cmd3),
            Some(("perse_bringup".to_string(), "calibrate.launch".to_string()))
        );

        // Com argumentos ROS depois do launchfile (ex: arg:=val)
        let cmd4 =
            "ros2 launch perse_bringup calibrate.launch use_sim_time:=true calibrate_gyro:=false";
        assert_eq!(
            parse_launch_command(cmd4),
            Some(("perse_bringup".to_string(), "calibrate.launch".to_string()))
        );

        // Caminho relativo no segundo argumento
        let cmd5 = "ros2 launch perse_bringup launch/calibrate.launch";
        assert_eq!(
            parse_launch_command(cmd5),
            Some(("perse_bringup".to_string(), "calibrate.launch".to_string()))
        );

        // Comando que NÃO é launch do ros2
        let cmd_non_launch = "ros2 topic echo /odom";
        assert_eq!(parse_launch_command(cmd_non_launch), None);

        // Processo de nó ROS filho gerado por launchfile
        let cmd_node = "/opt/ros/humble/lib/robot_state_publisher/robot_state_publisher --ros-args -r __node:=robot_state_publisher";
        assert_eq!(parse_launch_command(cmd_node), None);

        // Processo qualquer do sistema
        let cmd_sys = "bash -c sleep 100";
        assert_eq!(parse_launch_command(cmd_sys), None);
    }

    #[test]
    fn test_independent_status_detection() {
        // Quando perse.launch.xml roda, o comando registrado é apenas para ele
        let cmd_parent = "ros2 launch perse_bringup perse.launch.xml";
        let parsed = parse_launch_command(cmd_parent);
        assert_eq!(
            parsed,
            Some(("perse_bringup".to_string(), "perse.launch.xml".to_string()))
        );

        // Não deve dar match com um filho (ex: lsm9ds1_handler.launch.py)
        let child_file = "lsm9ds1_handler.launch.py";
        assert_ne!(parsed.as_ref().map(|(_, f)| f.as_str()), Some(child_file));
    }

    #[test]
    fn test_parse_ps_output() {
        let sample_ps = r#"
  PID ARGS
    1 /usr/lib/systemd/systemd
 1234 python3 /opt/ros/humble/bin/ros2 launch perse_bringup robot_bringup.launch.py
 5678 ros2 topic list
 9012 ros2 launch perse_bringup perse.launch.xml arg:=1
"#;
        let launches = parse_ps_output(sample_ps);
        assert_eq!(launches.len(), 2);
        assert_eq!(launches[0].pid, 1234);
        assert_eq!(launches[0].package, "perse_bringup");
        assert_eq!(launches[0].filename, "robot_bringup.launch.py");

        assert_eq!(launches[1].pid, 9012);
        assert_eq!(launches[1].package, "perse_bringup");
        assert_eq!(launches[1].filename, "perse.launch.xml");
    }
}
