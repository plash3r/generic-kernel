# Generic — основа ядра на Rust

Репозиторий: [plash3r/generic-kernel](https://github.com/plash3r/generic-kernel).
Название ядра: Generic. Первый этап разработки собственного модульного
монолитного ядра x86_64. Это ранняя основа, не готовая ОС и не production-ready ядро.
Лицензия пока не выбрана владельцем и намеренно не назначена.

## Реализовано

- Отдельный `no_std` ELF ядра и инструмент создания UEFI disk image.
- Загрузка через сторонний Rust OSDev bootloader 0.11.10.
- Диагностика COM1; вывод panic с ограниченным ожиданием UART.
- GDT, TSS, IDT; обработчики breakpoint, invalid opcode, GP, page fault,
  double fault с отдельным IST-стеком 32 KiB.
- Bootstrap-аллокатор физических страниц 4 KiB без динамической памяти:
  выравнивание, проверка карты, пропуск нулевой страницы, защита от переполнения.
- Проверка записи/чтения двух выделенных страниц через отображение памяти загрузчика.
- Проверка возврата из breakpoint; QEMU smoke runner с timeout и кодом завершения.
- Отдельная платформонезависимая библиотека с тестами; CI для сборки и загрузки.

## Пока отсутствует

Собственные таблицы страниц и heap, полный набор обработчиков исключений,
аппаратные IRQ и таймер, APIC/ACPI, SMP, процессы, ring 3, syscalls, ELF user loader,
VFS, накопители, сеть, клавиатура, экранная консоль и графический интерфейс.
После диагностики ядро останавливается через CLI/HLT. Обработчики IRQ ещё не готовы,
поэтому аппаратные прерывания намеренно не включаются.

## Сборка и запуск — Linux / WSL2

Установить Rust через rustup, C toolchain, Python 3, QEMU и OVMF.
Для Ubuntu: `sudo apt-get install build-essential pkg-config qemu-system-x86 ovmf`.
Rustup прочитает `rust-toolchain.toml` и установит закреплённый nightly и target.
Первой сборке нужен интернет для зависимостей.

```bash
cargo test --locked -p kernel-core
bash scripts/build.sh
python3 scripts/run.py
```

Вывод идёт в терминал через serial. Ожидаемая завершающая строка: `GENERIC: READY`.
Остановка обычного запуска — Ctrl+C. Образ лежит в `build/generic-uefi.img`.
Если прошивка не найдена, задать `OVMF_CODE=/absolute/path/OVMF_CODE_4M.fd`.
Запуск использует один CPU и TCG, не требует KVM, не подключает сеть и физические диски.

```bash
bash scripts/build.sh smoke
python3 scripts/run.py --smoke
```

Smoke-версия выходит из QEMU через порт 0xf4. Успех требует одновременно exit code 33
и маркер READY; panic, аварийное завершение или timeout считаются ошибкой.
Журнал: `build/smoke.log`. Обычный и smoke-образы сохраняются под разными именами.

## Структура

- `kernel/src/arch/` — привилегированный код x86_64, UART, таблицы дескрипторов.
- `kernel/src/main.rs` — последовательность загрузки и диагностики.
- `crates/kernel-core/` — безопасные алгоритмы без зависимости от платформы.
- `tools/image/` — создание загрузочного образа на хосте.
- `scripts/` — сборка и QEMU; `docs/` — решения, план и ограничения.

Для GDB собрать обычный образ и выполнить `python3 scripts/run.py --debug`.
Подключение: `target remote localhost:1234`, ELF: `target/x86_64-unknown-none/release/generic-kernel`.
Для полноценной отладки символов добавить `debug = 2` в `[profile.release]` и пересобрать.

Фактический результат проверок в этом окружении — `docs/VALIDATION.md`.

## Образы и CI

В Git хранятся исходники; `build/` и `target/` исключены. После клонирования
сначала выполните сборку. Образы из первоначального ZIP — отдельная поставка.
GitHub Actions собирает smoke-образ и запускает его в QEMU; результат смотрите
во вкладке Actions. Наличие workflow само по себе не означает успешный тест.
