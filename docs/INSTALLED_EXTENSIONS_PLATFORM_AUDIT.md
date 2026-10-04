# Дополнительный аудит установленных расширений

Дата: 2026-10-04. Выполнен после push основных исправлений Tezbar `8897ae2` и Google Translate `bbb1d75`.

## Объём и безопасность

Проверены ещё 8 пакетов: Audio Device, Speedtest, Google Search, Amphetamine, Timers, CleanShot X, Perplexity, Color Picker. Источник — установленные `.sc-build` bundles, а не upstream исходники: в этих пакетах нет `src`/Git repository.

Временные копии содержат только manifest, bundles и assets. Пользовательские credentials, LocalStorage, caches и native helpers не копировались. Установленные пакеты не изменялись. Не запускались тест скорости, аудиопереключение, реальные таймеры, screenshots/OCR, browser/app launch или изменение аккаунтов.

Это macOS backend/runtime-аудит и анализ Windows eligibility, **не Windows E2E**. Наличие API-экспорта само по себе не подтверждает всю его семантику или доступность native permissions.

## Найдено и исправлено в Tezbar

Первоначально: **15 passed / 6 failed** в 21 пробе.

1. `Alert.ActionStyle` отсутствовал в backend API, хотя использовался Speedtest, Timers, CleanShot X и Color Picker. Добавлены Default/Destructive/Cancel.
2. `getSelectedText` отсутствовал у Google Search/Color Picker (и auto-input Translate). Добавлен readonly accessibility-адаптер: macOS `AXSelectedText`, Windows UI Automation `TextPattern.GetSelection()`. Он не симулирует Ctrl+C и не заменяет clipboard. Отсутствующая selection/permissions возвращают понятную ошибку; record-mode не читает desktop selection. Windows текст кодируется base64, чтобы сохранить trailing whitespace. Native availability зависит от приложения и разрешений, проверена логика адаптеров с моками, не реальная Windows selection.
3. Старый Timers не имел `platforms`, хотя использовал POSIX shell, `afplay`, `say`, `osascript`. Раньше отсутствие списка означало совместимость с Windows. Теперь legacy manifests без декларации по умолчанию macOS-only; явные `platforms` или `tezbar.platforms` разрешают Windows. Невалидный/пустой список не обходит фильтр. Не выполнен ложный «порт» Timers: он остаётся macOS-пакетом и исключается из Windows discovery/install/build.

Повторная проверка: **21/21 passed**. Штатные unit-тесты дополнительно закрепляют selected-text контракты и правила платформенной совместимости.

## Результаты по пакетам

| Пакет         | Что действительно проверено                                                                          | Что ещё требует native/E2E                                                                        |
| ------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Audio Device  | Customize Order view и все используемые top-level API exports                                        | listing/select/volume, download/checksum native Windows CLI, permissions                          |
| Speedtest     | используемые API exports; код содержит отдельный Windows CLI archive path                            | download/extraction/CLI execution, multiline progress, реальное измерение скорости, Windows ARM64 |
| Google Search | стартовый пустой список без сети; API exports                                                        | suggestions через сеть; selected-text из реального приложения                                     |
| Amphetamine   | manifest macOS-only; API exports                                                                     | работа с настоящим Amphetamine и permissions; Windows не поддерживается намеренно                 |
| Timers        | Custom Timer и Configure Menubar Presets открываются; API exports; безопасная macOS-only eligibility | запуск/завершение timer, sound/notification; Windows-порт отсутствует                             |
| CleanShot X   | Manage Recording Presets view; Open History записывает URL без app launch; API exports               | работа с установленным CleanShot X; macOS-only по manifest                                        |
| Perplexity    | форма; переданный query порождает правильный browser URL в record-mode                               | app deep link/browser launch/fallback text                                                        |
| Color Picker  | Convert Color, Organize Colors, Color Wheel views; API exports                                       | native picker, extract-color/OCR/screenshots, selected-text из реального приложения               |

Нельзя считать все действия этих расширений проверенными: в частности, platform declaration не доказывает работоспособность native binary на Windows, а read-only API проверка не проверяет UI confirmation/actions и взаимодействие с внешними приложениями.

## Повторный запуск

Из `Tezbar`:

```sh
pnpm exec vitest run --config tests/manual/installed-extensions.vitest.config.ts
```

По умолчанию используется macOS install root `~/Library/Application Support/com.tezbar.app/extensions`. Для другой коллекции задайте `TEZBAR_EXTENSION_TEST_ROOT`. Ожидается наличие всех 8 перечисленных пакетов; пробы используют их копии и не модифицируют оригиналы.

Логи: `/tmp/tezbar-installed-extensions.log` (до) и `/tmp/tezbar-installed-extensions-after.log` (после).

## Статус доставки

- Основные изменения Tezbar запушены в `main` коммитом `8897ae2`; этот аудит и найденные исправления идут отдельным следующим коммитом.
- Google Translate: push `bbb1d75` и `c927819` (ожидание Windows MediaEnded), существующее изменение `package-lock.json` оставлено пользователю.
- VSCode Recent Projects: исправление сохранено локально в `3bc2818`. Remote отсутствует; репозитория `almatkai/visual-studio-code-recent-projects` на GitHub не найдено. Для push нужен предоставленный remote или разрешение создать репозиторий. Репозиторий автоматически не создавался.
