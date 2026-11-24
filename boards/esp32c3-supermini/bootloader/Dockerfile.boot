# Stock ESP-IDF second stage for ESP32-C3; externally signed together with
# the factory microtun boot application using the provisioned RSA-3072 key.
FROM espressif/idf:v5.5.5 AS builder
SHELL ["/bin/bash", "-lc"]
WORKDIR /project
# The selected profile becomes the actual sdkconfig.defaults before IDF runs.
# Always generate sdkconfig fresh: never reuse a config from another profile.
ARG ESP_BOOTLOADER_PROFILE=dev
# The board's flash size (4MB, 8MB or 16MB) and partition table (relative to
# the board directory). microtun-boot-tools-esp passes both from its board
# table; boards that share this project differ only in these two.
ARG ESP_FLASH_SIZE
ARG ESP_PARTITION_TABLE=layout/partitions.csv
COPY bootloader/CMakeLists.txt bootloader/sdkconfig.defaults.dev bootloader/sdkconfig.defaults.release ./
COPY ${ESP_PARTITION_TABLE} partitions.csv

RUN case "$ESP_BOOTLOADER_PROFILE" in dev|release) ;; \
        *) echo "Invalid ESP_BOOTLOADER_PROFILE: $ESP_BOOTLOADER_PROFILE" >&2; exit 2 ;; \
    esac && \
    case "$ESP_FLASH_SIZE" in 4MB|8MB|16MB) ;; \
        *) echo "Invalid ESP_FLASH_SIZE: '$ESP_FLASH_SIZE' (expected 4MB, 8MB or 16MB)" >&2; exit 2 ;; \
    esac && \
    cp "sdkconfig.defaults.$ESP_BOOTLOADER_PROFILE" sdkconfig.defaults && \
    echo "CONFIG_ESPTOOLPY_FLASHSIZE_$ESP_FLASH_SIZE=y" >> sdkconfig.defaults && \
    source "$IDF_PATH/export.sh" && \
    idf.py set-target esp32c3 && \
    idf.py bootloader && \
    test -s build/bootloader/bootloader.bin && \
    cp build/bootloader/bootloader.bin /bootloader.bin

FROM scratch AS artifact
COPY --from=builder /bootloader.bin /bootloader.bin
