# Фактические проверки — 2026-09-21

| Проверка | Результат |
|---|---|
| Компилятор | rustc 1.89.0-nightly, nightly-2025-06-01 |
| cargo fmt --all -- --check | PASS |
| cargo test --locked -p kernel-core | PASS: 5 тестов |
| Release kernel, x86_64-unknown-none, smoke и normal | PASS, без предупреждений |
| Создание smoke и normal UEFI images | PASS |
| Синтаксис scripts/build.sh и scripts/run.py | PASS |
| Запуск QEMU smoke | НЕ ВЫПОЛНЕН: QEMU не установлен |
| Проверка на реальном оборудовании | НЕ ВЫПОЛНЕНА |
| GitHub Actions | Подготовлен workflow; запуск не выполнялся |

Сборка образа не доказывает успешную загрузку. Самопроверки RAM и int3 написаны
и скомпилированы, но их выполнение ещё не наблюдалось. Первый обязательный шаг
после получения проекта — `python3 scripts/run.py --smoke` в Linux/WSL2 с QEMU и OVMF.
В первоначальном ZIP готовый образ находится в build/. После клонирования Git
образ нужно собрать: `bash scripts/build.sh smoke`.

Журналы: evidence/build.txt, evidence/qemu.txt, evidence/elf.txt.
SHA-256 образов и обычного ELF находятся в build/SHA256SUMS первоначального ZIP;
собранные файлы не включены в Git.

В этом окружении системная установка QEMU/OVMF завершилась ошибками прав доступа.
Снижение ограничений или обход контроля доступа не выполнялись.
