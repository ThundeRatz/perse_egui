# Makefile para automação de build, download e instalação do perse_egui
# ThundeRatz Trekking - Robô Perse

PREFIX ?= /usr/local
PORT ?= 8080
CONNECT ?= 127.0.0.1:9876
RELEASE_URL ?= https://github.com/ThundeRatz/perse_egui/releases/latest/download

# Detecção automática de arquitetura
ARCH := $(shell uname -m)
ifeq ($(ARCH),x86_64)
    TARGET_ARCH := x86_64
else ifeq ($(ARCH),aarch64)
    TARGET_ARCH := aarch64
else ifeq ($(ARCH),arm64)
    TARGET_ARCH := aarch64
else
    TARGET_ARCH := $(ARCH)
endif

.PHONY: all build build-web download install install-user daemon run service clean help

help:
	@echo "=========================================================================="
	@echo " perse_egui - Ferramentas de Build e Instalação (ThundeRatz)"
	@echo "=========================================================================="
	@echo "Comandos disponíveis:"
	@echo "  make all          - Compila o binário nativo release e os artefatos web"
	@echo "  make build        - Compila o binário nativo release com Cargo"
	@echo "  make build-web    - Compila os artefatos WebAssembly (executa build_web.sh)"
	@echo "  make download     - Baixa binário pré-compilado ($(TARGET_ARCH)) e pasta web"
	@echo "                      (ideal para o robô/Jetson sem cargo instalado)"
	@echo "  make install      - Instala binário e pasta web em $(PREFIX) (requer sudo)"
	@echo "  make install-user - Instala binário em ~/.local/bin sem precisar de sudo"
	@echo "  make daemon       - Executa localmente em modo servidor daemon na porta $(PORT)"
	@echo "  make run          - Executa a interface nativa conectando em $(CONNECT)"
	@echo "  make service      - Cria e configura serviço systemd para rodar daemon no boot"
	@echo "  make clean        - Limpa os artefatos compilados"
	@echo "=========================================================================="

all: build build-web

build:
	@echo "==> Compilando binário nativo com Cargo em modo release..."
	cargo build --release

build-web:
	@echo "==> Compilando artefatos WebAssembly..."
	chmod +x ./build_web.sh
	./build_web.sh

download:
	@echo "==> Arquitetura detectada: $(TARGET_ARCH)"
	@mkdir -p target/release
	@echo "==> Baixando binário pré-compilado perse_egui-$(TARGET_ARCH)..."
	curl -fsSL -o target/release/perse_egui "$(RELEASE_URL)/perse_egui-$(TARGET_ARCH)" || \
		(echo "❌ Erro ao baixar perse_egui-$(TARGET_ARCH) de $(RELEASE_URL)" && exit 1)
	@chmod +x target/release/perse_egui
	@echo "==> Baixando artefatos WebAssembly (perse_egui-web.tar.gz)..."
	curl -fsSL -o perse_egui-web.tar.gz "$(RELEASE_URL)/perse_egui-web.tar.gz" || \
		(echo "❌ Erro ao baixar perse_egui-web.tar.gz de $(RELEASE_URL)" && exit 1)
	@tar -xzf perse_egui-web.tar.gz
	@rm -f perse_egui-web.tar.gz
	@echo "✅ Download concluído! Binário em 'target/release/perse_egui' e pasta 'web/' pronta."

install:
	@echo "==> Instalando em $(PREFIX)..."
	@if [ ! -f target/release/perse_egui ]; then \
		echo "⚠️ Binário não encontrado em target/release/perse_egui. Executando 'make download' primeiro..."; \
		$(MAKE) download; \
	fi
	install -d $(PREFIX)/bin
	install -m 755 target/release/perse_egui $(PREFIX)/bin/perse_egui
	install -d $(PREFIX)/share/perse_egui/web
	cp -r web/* $(PREFIX)/share/perse_egui/web/
	@echo "✅ Instalação concluída com sucesso em $(PREFIX)/bin/perse_egui!"

install-user:
	@echo "==> Instalando para o usuário atual em $(HOME)/.local..."
	@if [ ! -f target/release/perse_egui ]; then \
		echo "⚠️ Binário não encontrado. Executando 'make download' primeiro..."; \
		$(MAKE) download; \
	fi
	install -d $(HOME)/.local/bin
	install -m 755 target/release/perse_egui $(HOME)/.local/bin/perse_egui
	install -d $(HOME)/.local/share/perse_egui/web
	cp -r web/* $(HOME)/.local/share/perse_egui/web/
	@echo "✅ Instalação de usuário concluída em $(HOME)/.local/bin/perse_egui!"
	@echo "Certifique-se de que $(HOME)/.local/bin está no seu PATH."

daemon:
	@if [ -f $(PREFIX)/bin/perse_egui ]; then \
		$(PREFIX)/bin/perse_egui --daemon --port $(PORT) --connect $(CONNECT); \
	elif [ -f target/release/perse_egui ]; then \
		target/release/perse_egui --daemon --port $(PORT) --connect $(CONNECT); \
	else \
		echo "Binário não encontrado. Execute 'make build' ou 'make download' primeiro."; \
		exit 1; \
	fi

run:
	@if [ -f target/release/perse_egui ]; then \
		target/release/perse_egui --connect $(CONNECT); \
	else \
		cargo run --release -- --connect $(CONNECT); \
	fi

service:
	@echo "==> Configurando serviço systemd perse_egui.service..."
	@sudo bash -c 'cat <<EOF > /etc/systemd/system/perse_egui.service\n[Unit]\nDescription=Perse Egui Web Daemon Server\nAfter=network.target\n\n[Service]\nType=simple\nUser=$(shell whoami)\nExecStart=$(PREFIX)/bin/perse_egui --daemon --port $(PORT) --connect $(CONNECT)\nRestart=always\nRestartSec=3\n\n[Install]\nWantedBy=multi-user.target\nEOF'
	@sudo systemctl daemon-reload
	@echo "✅ Serviço criado em /etc/systemd/system/perse_egui.service!"
	@echo "Para iniciar agora: sudo systemctl enable --now perse_egui"
	@echo "Para ver status:    sudo systemctl status perse_egui"

clean:
	cargo clean
	rm -f perse_egui-web.tar.gz
