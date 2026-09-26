# Integreção do `rewire-viewer` no `perse_egui`

## Visão Geral

O `perse_egui` é uma aplicação Rust baseada em `egui`/`eframe` que incorpora o `rewire-viewer` como submódulo Git na pasta `external/rewire-viewer`.

## Commit Upstream Utilizado

Submódulo: `external/rewire-viewer`
Repositório: `https://github.com/rewire-run/viewer.git`
Commit: `1d12b56cd04f4d86d35fa76ed568c5faba4aee25` (tag v0.5.0-32-g1d12b56)

## Arquitetura de Integração

Toda a interação com a biblioteca `rewire-viewer` está encapsulada no módulo `src/rewire_integration.rs`. O restante da aplicação interage com o viewer através da seguinte interface:

- `RewireIntegration::create(cc, endpoint)`: Inicializa o viewer, cria o link do relay e os serviços de controle e registra as views customizadas.
- `RewireIntegration::logic(ctx, frame)`: Executa a lógica por frame do viewer (chamado no início de `eframe::App::update`).
- `RewireIntegration::show(ui, frame)`: Desenha a interface do viewer no `CentralPanel`.
- `RewireIntegration::save(storage)`: Persiste o estado do Rerun/viewer.

## Atualizando o Submódulo

Para atualizar o submódulo no futuro:

1. Crie uma branch de atualização:
   ```bash
   git checkout -b update-rewire-viewer
   ```
2. Atualize o submódulo:
   ```bash
   cd external/rewire-viewer
   git fetch origin --tags
   git checkout <novo-commit-ou-tag>
   cd ../..
   ```
3. Execute a validação completa:
   ```bash
   cargo check --all-targets
   cargo test --all-targets
   cargo clippy --all-targets --all-features -- -D warnings
   cargo fmt --all -- --check
   cargo build --release
   ```
4. Se necessário, adapte apenas o código em `src/rewire_integration.rs`.
5. Faça o commit das alterações incluindo `external/rewire-viewer` e `Cargo.lock`.
