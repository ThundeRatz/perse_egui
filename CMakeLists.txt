cmake_minimum_required(VERSION 3.12)
project(perse_egui VERSION 0.1.0)

find_package(ament_cmake REQUIRED)

# Opção para forçar o download dos binários pré-compilados (padrão no robô/Jetson)
option(PERSE_DOWNLOAD_PREBUILT "Forçar download dos binários pré-compilados do GitHub Releases em vez de compilar" OFF)
option(PERSE_BUILD_DAEMON_ONLY "Compilar apenas modo headless daemon (--no-default-features)" OFF)

# Verificar se o cargo está disponível no PATH do sistema
find_program(CARGO_EXECUTABLE cargo)

# Identificar a arquitetura do sistema
string(TOLOWER "${CMAKE_SYSTEM_PROCESSOR}" SYSTEM_ARCH)
if(SYSTEM_ARCH MATCHES "x86_64" OR SYSTEM_ARCH MATCHES "amd64")
    set(TARGET_ARCH "x86_64")
elseif(SYSTEM_ARCH MATCHES "aarch64" OR SYSTEM_ARCH MATCHES "arm64")
    set(TARGET_ARCH "aarch64")
elseif(SYSTEM_ARCH MATCHES "armv7" OR SYSTEM_ARCH MATCHES "armhf")
    set(TARGET_ARCH "armv7")
else()
    set(TARGET_ARCH "${SYSTEM_ARCH}")
endif()

set(PERSE_EGUI_BINARY "")

if(CARGO_EXECUTABLE AND NOT PERSE_DOWNLOAD_PREBUILT)
    message(STATUS "Cargo encontrado (${CARGO_EXECUTABLE}). Compilando perse_egui com Cargo...")

    set(CARGO_OUTPUT_DIR "${CMAKE_CURRENT_SOURCE_DIR}/target/release")
    set(PERSE_EGUI_BINARY "${CARGO_OUTPUT_DIR}/perse_egui")

    if(PERSE_BUILD_DAEMON_ONLY)
        message(STATUS "PERSE_BUILD_DAEMON_ONLY ativado: Compilando com --no-default-features")
        set(CARGO_EXTRA_ARGS "--no-default-features")
    else()
        set(CARGO_EXTRA_ARGS "")
    endif()

    add_custom_target(perse_egui_cargo ALL
        COMMAND ${CARGO_EXECUTABLE} build --release ${CARGO_EXTRA_ARGS}
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
        COMMENT "Compilando binário perse_egui com Cargo"
    )
else()
    if(PERSE_DOWNLOAD_PREBUILT)
        message(STATUS "PERSE_DOWNLOAD_PREBUILT ativado. Baixando e instalando release pré-compilada via download_release.sh...")
    else()
        message(STATUS "Cargo não encontrado no PATH. Baixando e instalando release pré-compilada via download_release.sh...")
    endif()

    set(PERSE_EGUI_BINARY "${CMAKE_CURRENT_BINARY_DIR}/perse_egui")

    execute_process(
        COMMAND bash "${CMAKE_CURRENT_SOURCE_DIR}/download_release.sh" "${CMAKE_CURRENT_BINARY_DIR}"
        RESULT_VARIABLE DOWNLOAD_RESULT
        WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    )

    if(NOT DOWNLOAD_RESULT EQUAL 0)
        message(WARNING "download_release.sh retornou erro (${DOWNLOAD_RESULT}).")
        if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/target/release/perse_egui")
            message(STATUS "Usando binário existente em target/release/perse_egui como fallback.")
            set(PERSE_EGUI_BINARY "${CMAKE_CURRENT_SOURCE_DIR}/target/release/perse_egui")
        elseif(EXISTS "$ENV{HOME}/.local/bin/perse_egui")
            message(STATUS "Usando binário existente em ~/.local/bin/perse_egui como fallback.")
            set(PERSE_EGUI_BINARY "$ENV{HOME}/.local/bin/perse_egui")
        else()
            message(WARNING "Binário perse_egui não encontrado. Certifique-se de ter conexão com a internet ou instale o binário manualmente.")
        endif()
    endif()
endif()

# Instalar o executável em lib/${PROJECT_NAME}/perse_egui
if(EXISTS "${PERSE_EGUI_BINARY}")
    install(PROGRAMS ${PERSE_EGUI_BINARY}
        DESTINATION lib/${PROJECT_NAME}
    )
endif()

# Instalar pasta launch em share/${PROJECT_NAME}/launch
install(DIRECTORY launch/
    DESTINATION share/${PROJECT_NAME}/launch
)

# Instalar pasta web em share/${PROJECT_NAME}/web para o modo daemon
if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/web/rewire_viewer_bg.wasm")
    install(DIRECTORY web/
        DESTINATION share/${PROJECT_NAME}/web
    )
elseif(EXISTS "${CMAKE_CURRENT_BINARY_DIR}/web")
    install(DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/web/
        DESTINATION share/${PROJECT_NAME}/web
    )
elseif(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/web")
    install(DIRECTORY web/
        DESTINATION share/${PROJECT_NAME}/web
    )
endif()

ament_package()
