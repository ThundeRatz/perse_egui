#!/usr/bin/env bash
set -euo pipefail

echo "======================================================="
echo " Compilando Rewire Viewer para WebAssembly (wasm32)    "
echo "======================================================="

# 1. Garantir que o target wasm32-unknown-unknown está instalado
echo "==> Adicionando target wasm32-unknown-unknown..."
rustup target add wasm32-unknown-unknown

# 2. Verificar/Instalar wasm-bindgen-cli e wasm-opt
if ! command -v wasm-bindgen &> /dev/null || ! wasm-bindgen --version | grep -q "0.2.126"; then
    echo "==> Instalando/Atualizando wasm-bindgen-cli (0.2.126)..."
    cargo install -f wasm-bindgen-cli --version 0.2.126
fi

if ! command -v wasm-opt &> /dev/null; then
    echo "==> wasm-opt não encontrado. Tentando instalar via cargo-binstall..."
    if ! command -v cargo-binstall &> /dev/null; then
        curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
    fi
    cargo binstall --no-confirm wasm-opt || echo "⚠️ Não foi possível instalar wasm-opt automaticamente. A etapa de pós-otimização será ignorada caso não esteja no PATH."
fi

# 3. Otimizar dependências do visualizador (desabilitar mcap/parquet para WASM)
if [ -f external/rewire-viewer/Cargo.toml ]; then
    sed -i 's/features = \["all"\]/default-features = false, features = ["image"]/' external/rewire-viewer/Cargo.toml 2>/dev/null || true
fi

# 4. Compilar o pacote perse_egui para target wasm32
echo "==> Compilando o pacote perse_egui em release para WASM..."
RUSTFLAGS=--cfg=web_sys_unstable_apis cargo build --target wasm32-unknown-unknown --release --lib
WASMBIN="target/wasm32-unknown-unknown/release/perse_egui.wasm"

# 5. Gerar o glue JS e o arquivo binário .wasm na pasta web/
echo "==> Gerando os artefatos rewire_viewer.js e rewire_viewer_bg.wasm em web/..."
mkdir -p web/
wasm-bindgen --target web --out-dir web/ --out-name rewire_viewer "$WASMBIN"

# 6. Otimizar tamanho do arquivo WebAssembly com wasm-opt
if command -v wasm-opt &> /dev/null; then
    echo "==> Executando wasm-opt -O2 para reduzir o tamanho do binário WebAssembly..."
    wasm-opt -O2 web/rewire_viewer_bg.wasm -o web/rewire_viewer_bg.wasm
else
    echo "⚠️ wasm-opt não disponível. Mantendo o arquivo .wasm sem a otimização secundária."
fi

echo "======================================================="
echo " Artefatos gerados com sucesso na pasta web/:"
ls -lh web/rewire_viewer*
echo "======================================================="
