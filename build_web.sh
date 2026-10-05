#!/usr/bin/env bash
set -euo pipefail

echo "======================================================="
echo " Compilando Rewire Viewer para WebAssembly (wasm32)    "
echo "======================================================="

# 1. Garantir que o target wasm32-unknown-unknown está instalado
echo "==> Adicionando target wasm32-unknown-unknown..."
rustup target add wasm32-unknown-unknown

# 2. Verificar/Instalar wasm-bindgen-cli
if ! command -v wasm-bindgen &> /dev/null || ! wasm-bindgen --version | grep -q "0.2.126"; then
    echo "==> Instalando/Atualizando wasm-bindgen-cli (0.2.126)..."
    cargo install -f wasm-bindgen-cli --version 0.2.126
fi

# 3. Compilar o pacote perse_egui para target wasm32
echo "==> Compilando o pacote perse_egui em release para WASM..."
RUSTFLAGS=--cfg=web_sys_unstable_apis cargo build --target wasm32-unknown-unknown --release --lib
WASMBIN="target/wasm32-unknown-unknown/release/perse_egui.wasm"

# 4. Gerar o glue JS e o arquivo binário .wasm na pasta web/
echo "==> Gerando os artefatos rewire_viewer.js e rewire_viewer_bg.wasm em web/..."
mkdir -p web/
wasm-bindgen --target web --out-dir web/ --out-name rewire_viewer "$WASMBIN"

echo "======================================================="
echo " Artefatos gerados com sucesso na pasta web/:"
ls -lh web/rewire_viewer*
echo "======================================================="
