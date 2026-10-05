<h1 align="center">
    perse_egui - ThundeRatz Trekking
</h1>

<p align="center">
<img src="https://img.shields.io/badge/ROS%20version-humble-informational?style=for-the-badge" href="http://wiki.ros.org/humble"/>
<img src="https://img.shields.io/badge/Rust-2021-orange?style=for-the-badge&logo=rust" href="https://www.rust-lang.org/"/>
<img src="https://img.shields.io/badge/WebAssembly-WASM-purple?style=for-the-badge&logo=webassembly" href="https://webassembly.org/"/>
<img src="https://img.shields.io/badge/egui-0.36-blue?style=for-the-badge" href="https://github.com/emilk/egui"/>
</p>

<p align="center">
<img src="https://forthebadge.com/images/badges/made-with-rust.svg" href="https://forthebadge.com"/>
<img src="https://forthebadge.com/images/badges/built-with-love.svg" href="https://forthebadge.com"/>
</p>

O **perse_egui** é a interface gráfica de controle, telemetria, diagnóstico e planejamento de missões desenvolvida para o robô autônomo **Perse**, competidor da categoria **Trekking** pela equipe **ThundeRatz**.

O projeto reúne, em uma única aplicação acelerada por hardware (nativa e WebAssembly):
- **Visualizador 3D/2D (Rewire / Rerun Viewer)**: monitoramento em tempo real de sensores (câmera OAK-D, LIDAR), odometria, mapas e transformadas (TF) através de conexão com o relay rewire.
- **Editor 2D de Missões**: planejamento e ajuste de trajetórias em campo gramado, posicionamento de marcos (waypoints) com atributos de controle (velocidades, margens, desvio de cone) e delimitação de obstáculos.
- **Painéis de Controle Remoto**: gerenciamento de parâmetros ROS 2 em tempo real, inicialização/interrupção de launchfiles e monitoramento de terminais no robô.

<p align="center">
  <img src="./.readme/rerun.png" alt="Visualizador Rerun e Rewire" width="49%">
  <img src="./.readme/pontos.png" alt="Editor de Missão 2D" width="49%">
</p>

---

## :sparkles: Funcionalidades

### 🎯 Editor de Missões 2D
- **Criação e Edição de Marcos**: adicione pontos de navegação clicando no plano cartesiano, ajuste coordenadas, velocidades de odometria/visão e margens de aceitação.
- **Obstáculos Geométricos**: suporte a polígonos, linhas, retângulos e círculos (físicos ou cosméticos).
- **Múltiplos Conjuntos de Missão**: crie, duplique, renomeie e alterne entre diferentes estratégias de pista sem sobrescrever arquivos.
- **Persistência Completa**: salvamento em formato padrão YAML (`mission_points.yaml`) compatível com os nós de navegação do robô e sincronização contínua via WebSocket.
- **Atalhos Rápidos**: suporte a `Ctrl + S` para salvar, `Delete` / `Backspace` para exclusão e arrasto interativo de pontos.

### 👁️ Visualizador Rewire / Rerun Integrado
- Conexão direta com streams gRPC/HTTP do rewire.
- Visualização de nuvens de pontos, mapas de ocupação, poses do robô e imagens da câmera estéreo.

### ⚙️ Painel de Parâmetros
- Navegação em árvore por pacotes e nós do ROS 2.
- Edição de valores numéricos, booleanos e strings com botão de aplicação remota (`apply_params`).

### 🚀 Launchfiles & Terminais
- Listagem remota dos launchfiles disponíveis na workspace do robô.
- Início e parada de rotinas com indicação de status em tempo real.
- Visualização e envio de comandos aos terminais remotos do robô.

---

## :globe_with_meridians: Modos de Operação

O **perse_egui** foi projetado para rodar em duas arquiteturas complementares:

1. **Modo Nativo (Desktop)**: Executável compilado para Linux/macOS/Windows com aceleração WGPU. Ideal para desenvolvimento local na estação de trabalho ou controle de campo com baixa latência.
2. **Modo Web & Daemon (No Robô / Navegador)**: O robô executa o binário com a flag `--daemon`. Ele disponibiliza um servidor HTTP/WebSocket com proxy reverso e serve a interface WebAssembly para que qualquer dispositivo (computador, tablet ou celular conectado à rede do robô) acesse o painel sem instalar dependências.

---

## :wrench: Pré-requisitos

### Compilação Nativa (Rust)
Para compilar o projeto diretamente pelo Rust, instale a toolchain oficial:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

Dependências de sistema para o backend gráfico no Ubuntu/Debian:
```bash
sudo apt update && sudo apt install -y \
    libclang-dev \
    libssl-dev \
    libxcb-render0-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libxkbcommon-dev \
    libasound2-dev \
    libfontconfig1-dev
```

### Compilação WebAssembly
Caso deseje gerar os artefatos WebAssembly para o navegador, instale o target `wasm32` e o utilitário `wasm-bindgen-cli`:

```bash
rustup target add wasm32-unknown-unknown
cargo install -f wasm-bindgen-cli
```

---

## :rocket: Como Executar

### 1. Aplicação Nativa (Desktop)

Para executar a interface nativa conectando-se ao rewire local:

```bash
cargo run --release
```

Para conectar-se a um robô remoto na rede (ex: Jetson do Perse no IP `192.168.0.100`):

```bash
cargo run --release -- --connect 192.168.0.100:9876
```

### 2. Modo Daemon (Servidor no Robô)

No robô (Jetson), inicie o `perse_egui` em modo servidor/daemon:

```bash
cargo run --release -- --daemon --port 8080 --connect 127.0.0.1:9876
```

Com o daemon rodando:
- O servidor HTTP serve os arquivos da pasta `web/` na porta `8080`.
- O endpoint WebSocket de controle fica em `/ws/control`.
- As requisições de telemetria gRPC/rewire são encaminhadas transparentemente através de `/proxy`.

### 3. Compilando os Artefatos Web (WASM)

Para atualizar os arquivos WebAssembly em `web/`:

```bash
./build_web.sh
```

Depois, basta acessar `http://localhost:8080` (ou o IP do robô na porta `8080`) pelo navegador.

---

## :package: Integração com ROS 2 (Colcon)

O `perse_egui` é estruturado como um pacote ROS 2 `ament_cmake`. Ele pode ser colocado dentro do diretório `src/` da sua workspace do ROS 2 (`perse_ws/src/perse_egui`).

### Compilando via Colcon

Na raiz da sua workspace do ROS 2:

```bash
colcon build --packages-select perse_egui
source install/setup.bash
```

> **Dica para computadores com limitação de memória RAM**: caso a compilação do Rust e de pacotes C++ sature os recursos, limite as threads de build:
> ```bash
> export MAKEFLAGS="-j 2"
> export CARGO_BUILD_JOBS=2
> colcon build --packages-select perse_egui --executor sequential
> ```

### Executando pelo ROS 2

Com o setup da workspace carregado:

```bash
# Executar diretamente o nó
ros2 run perse_egui perse_egui

# Ou através do arquivo de launch
ros2 launch perse_egui perse_egui.launch
```

---

## :floppy_disk: Persistência de Dados

- **Arquivo YAML da Missão**: o arquivo configurado no campo de texto (padrão: `mission_points.yaml`) contém a lista de pontos e obstáculos no formato aceito pelo robô.
- **Coleção de Conjuntos (`mission_sets.json`)**: armazena todos os conjuntos de missões criados, duplicados e renomeados no editor, garantindo que nenhum histórico seja perdido ao fechar o programa ou reiniciar o robô.
- **Storage da Aplicação**: preferências como largura das barras laterais, zoom, tema e opções de visibilidade da grade/margens são mantidas automaticamente pelo `eframe` nativo e pelo `localStorage` do navegador.
