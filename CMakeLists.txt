cmake_minimum_required(VERSION 3.12)
project(perse_egui VERSION 0.1.0)

find_package(ament_cmake REQUIRED)

# Opção para forçar o download dos binários pré-compilados (padrão no robô/Jetson)
option(PERSE_DOWNLOAD_PREBUILT "Forçar download dos binários pré-compilados do GitHub Releases em vez de compilar" OFF)

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

    add_custom_target(perse_egui_cargo ALL
        COMMAND ${CARGO_EXECUTABLE} build --release
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
        COMMENT "Compilando binário perse_egui com Cargo"
    )
else()
    if(PERSE_DOWNLOAD_PREBUILT)
        message(STATUS "PERSE_DOWNLOAD_PREBUILT ativado. Baixando binário pré-compilado para ${TARGET_ARCH}...")
    else()
        message(STATUS "Cargo não encontrado no PATH. Baixando binário pré-compilado para ${TARGET_ARCH}...")
    endif()

    set(DOWNLOAD_BINARY_NAME "perse_egui-${TARGET_ARCH}")
    set(DOWNLOAD_URL "https://github.com/ThundeRatz/perse_egui/releases/latest/download/${DOWNLOAD_BINARY_NAME}")
    set(PERSE_EGUI_BINARY "${CMAKE_CURRENT_BINARY_DIR}/perse_egui")

    message(STATUS "URL de Download do Binário: ${DOWNLOAD_URL}")
    message(STATUS "Destino: ${PERSE_EGUI_BINARY}")

    # Baixar binário pré-compilado
    file(DOWNLOAD
        "${DOWNLOAD_URL}"
        "${PERSE_EGUI_BINARY}"
        SHOW_PROGRESS
        STATUS DOWNLOAD_STATUS
    )

    list(GET DOWNLOAD_STATUS 0 STATUS_CODE)
    list(GET DOWNLOAD_STATUS 1 ERROR_MESSAGE)

    if(NOT STATUS_CODE EQUAL 0)
        message(WARNING "Não foi possível baixar o binário de ${DOWNLOAD_URL}: ${ERROR_MESSAGE}")
        # Tentar usar binário existente localmente como fallback
        if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/target/release/perse_egui")
            message(STATUS "Usando binário existente em target/release/perse_egui como fallback.")
            set(PERSE_EGUI_BINARY "${CMAKE_CURRENT_SOURCE_DIR}/target/release/perse_egui")
        else()
            message(WARNING "Certifique-se de que a máquina possui conexão à internet ou que o binário foi publicado no GitHub.")
        endif()
    else()
        file(CHMOD "${PERSE_EGUI_BINARY}" PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE GROUP_READ GROUP_EXECUTE WORLD_READ WORLD_EXECUTE)
    endif()

    # Baixar artefatos WebAssembly se não existirem localmente
    if(NOT EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/web/rewire_viewer_bg.wasm" AND NOT EXISTS "${CMAKE_CURRENT_BINARY_DIR}/web/rewire_viewer_bg.wasm")
        set(WEB_DOWNLOAD_URL "https://github.com/ThundeRatz/perse_egui/releases/latest/download/perse_egui-web.tar.gz")
        set(WEB_TARBALL "${CMAKE_CURRENT_BINARY_DIR}/perse_egui-web.tar.gz")
        message(STATUS "Baixando artefatos WebAssembly de ${WEB_DOWNLOAD_URL}...")

        file(DOWNLOAD
            "${WEB_DOWNLOAD_URL}"
            "${WEB_TARBALL}"
            SHOW_PROGRESS
            STATUS WEB_DOWNLOAD_STATUS
        )

        list(GET WEB_DOWNLOAD_STATUS 0 WEB_STATUS_CODE)
        if(WEB_STATUS_CODE EQUAL 0)
            message(STATUS "Extraindo artefatos web em ${CMAKE_CURRENT_BINARY_DIR}...")
            execute_process(
                COMMAND ${CMAKE_COMMAND} -E tar xzf "${WEB_TARBALL}"
                WORKING_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}"
            )
        else()
            message(WARNING "Não foi possível baixar os artefatos WebAssembly: ${WEB_DOWNLOAD_STATUS}")
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
