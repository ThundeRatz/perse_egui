#!/usr/bin/env bash
# Script para baixar a release mais recente do perse_egui e instalar em ~/.local/bin e ~/.local/share
# ThundeRatz Trekking - Robô Perse
set -euo pipefail

DEST_BIN_DIR="${HOME}/.local/bin"
DEST_SHARE_DIR="${HOME}/.local/share/perse_egui"
TARGET_DIR="${1:-}" # Diretório opcional para cópia adicional (ex: target/release ou build dir do CMake)

# 1. Detectar arquitetura
ARCH="$(uname -m)"
case "${ARCH}" in
    x86_64|amd64)
        TARGET_ARCH="x86_64"
        ;;
    aarch64|arm64)
        TARGET_ARCH="aarch64"
        ;;
    armv7*|armhf)
        TARGET_ARCH="armv7"
        ;;
    *)
        TARGET_ARCH="${ARCH}"
        ;;
esac

echo "======================================================="
echo " perse_egui - Download de Release Pré-compilada"
echo " Arquitetura detectada: ${TARGET_ARCH}"
echo "======================================================="

# 2. Obter a tag da release mais recente (fallback para v0.0.2 se API falhar)
LATEST_TAG=""
if command -v curl &>/dev/null; then
    LATEST_TAG=$(curl -fsSL https://api.github.com/repos/ThundeRatz/perse_egui/releases 2>/dev/null | grep -m1 '"tag_name":' | sed -E 's/.*"tag_name":[[:space:]]*"([^"]+)".*/\1/' || true)
fi

if [ -z "${LATEST_TAG}" ]; then
    LATEST_TAG="v0.0.2"
fi

BASE_URL="https://github.com/ThundeRatz/perse_egui/releases/download/${LATEST_TAG}"
BINARY_NAME="perse_egui-${TARGET_ARCH}"
TARBALL_NAME="perse_egui-web.tar.gz"

echo "==> Versão selecionada: ${LATEST_TAG}"
echo "==> Baixando de: ${BASE_URL}"

# 3. Criar diretórios de destino
mkdir -p "${DEST_BIN_DIR}"
mkdir -p "${DEST_SHARE_DIR}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

# 4. Baixar binário
echo "==> Baixando executável ${BINARY_NAME}..."
if ! curl -fL --progress-bar -o "${TMP_DIR}/perse_egui" "${BASE_URL}/${BINARY_NAME}"; then
    # Fallback para releases/latest/download caso o repositório publique uma release não-prerelease no futuro
    echo "⚠️ Falha ao baixar de ${BASE_URL}/${BINARY_NAME}. Tentando latest/download..."
    curl -fL --progress-bar -o "${TMP_DIR}/perse_egui" "https://github.com/ThundeRatz/perse_egui/releases/latest/download/${BINARY_NAME}"
fi
chmod +x "${TMP_DIR}/perse_egui"

# 5. Baixar arquivos WebAssembly
echo "==> Baixando artefatos web (${TARBALL_NAME})..."
if ! curl -fL --progress-bar -o "${TMP_DIR}/${TARBALL_NAME}" "${BASE_URL}/${TARBALL_NAME}"; then
    echo "⚠️ Falha ao baixar web de ${BASE_URL}/${TARBALL_NAME}. Tentando latest/download..."
    curl -fL --progress-bar -o "${TMP_DIR}/${TARBALL_NAME}" "https://github.com/ThundeRatz/perse_egui/releases/latest/download/${TARBALL_NAME}"
fi

# 6. Extrair e instalar artefatos web em ~/.local/share/perse_egui/web
echo "==> Instalando artefatos web em ${DEST_SHARE_DIR}/web..."
mkdir -p "${TMP_DIR}/extracted_web"
tar -xzf "${TMP_DIR}/${TARBALL_NAME}" -C "${TMP_DIR}/extracted_web"

mkdir -p "${DEST_SHARE_DIR}/web"
if [ -d "${TMP_DIR}/extracted_web/web" ]; then
    cp -r "${TMP_DIR}/extracted_web/web/"* "${DEST_SHARE_DIR}/web/"
else
    cp -r "${TMP_DIR}/extracted_web/"* "${DEST_SHARE_DIR}/web/"
fi

# 7. Instalar binário em ~/.local/bin
echo "==> Instalando binário em ${DEST_BIN_DIR}/perse_egui..."
cp "${TMP_DIR}/perse_egui" "${DEST_BIN_DIR}/perse_egui"

# 8. Se um diretório de destino adicional foi informado (ex: chamado pelo CMake ou Makefile)
if [ -n "${TARGET_DIR}" ]; then
    echo "==> Copiando também para o diretório alvo: ${TARGET_DIR}..."
    mkdir -p "${TARGET_DIR}"
    cp "${DEST_BIN_DIR}/perse_egui" "${TARGET_DIR}/perse_egui"
    mkdir -p "${TARGET_DIR}/web"
    cp -r "${DEST_SHARE_DIR}/web/"* "${TARGET_DIR}/web/"
fi

echo "======================================================="
echo "✅ Instalação concluída com sucesso!"
echo "   Binário: ${DEST_BIN_DIR}/perse_egui"
echo "   Web assets: ${DEST_SHARE_DIR}/web"
echo "======================================================="
