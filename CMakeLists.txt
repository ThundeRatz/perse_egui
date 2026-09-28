cmake_minimum_required(VERSION 3.12)
project(perse_egui VERSION 0.1.0)

find_package(ament_cmake REQUIRED)

# Verificar se o cargo está disponível no PATH do sistema
find_program(CARGO_EXECUTABLE cargo)

if(CARGO_EXECUTABLE)
    message(STATUS "Cargo encontrado (${CARGO_EXECUTABLE}). Compilando perse_egui com Cargo...")

    set(CARGO_OUTPUT_DIR "${CMAKE_CURRENT_SOURCE_DIR}/target/release")
    set(PERSE_EGUI_BINARY "${CARGO_OUTPUT_DIR}/perse_egui")

    add_custom_target(perse_egui_cargo ALL
        COMMAND ${CARGO_EXECUTABLE} build --release
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
        COMMENT "Compilando binário perse_egui com Cargo"
    )
else()
    message(STATUS "Cargo não encontrado no PATH. Baixando binário pré-compilado para a arquitetura atual...")

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

    set(DOWNLOAD_BINARY_NAME "perse_egui-${TARGET_ARCH}")
    set(DOWNLOAD_URL "https://github.com/ThundeRatz/perse_egui/releases/latest/download/${DOWNLOAD_BINARY_NAME}")
    set(PERSE_EGUI_BINARY "${CMAKE_CURRENT_BINARY_DIR}/perse_egui")

    message(STATUS "URL de Download: ${DOWNLOAD_URL}")
    message(STATUS "Destino: ${PERSE_EGUI_BINARY}")

    file(DOWNLOAD
        "${DOWNLOAD_URL}"
        "${PERSE_EGUI_BINARY}"
        SHOW_PROGRESS
        STATUS DOWNLOAD_STATUS
    )

    list(GET DOWNLOAD_STATUS 0 STATUS_CODE)
    list(GET DOWNLOAD_STATUS 1 ERROR_MESSAGE)

    if(NOT STATUS_CODE EQUAL 0)
        message(WARNING "Não foi possível baixar o binário pré-compilado de ${DOWNLOAD_URL}: ${ERROR_MESSAGE}")
        message(WARNING "Certifique-se de que os assets de release foram publicados no repositório ThundeRatz/perse_egui.")
    else()
        file(CHMOD "${PERSE_EGUI_BINARY}" PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE GROUP_READ GROUP_EXECUTE WORLD_READ WORLD_EXECUTE)
    endif()
endif()

# Instalar o executável em lib/${PROJECT_NAME}/perse_egui
install(PROGRAMS ${PERSE_EGUI_BINARY}
    DESTINATION lib/${PROJECT_NAME}
)

# Instalar pasta launch em share/${PROJECT_NAME}/launch
install(DIRECTORY launch/
    DESTINATION share/${PROJECT_NAME}/launch
)

ament_package()
