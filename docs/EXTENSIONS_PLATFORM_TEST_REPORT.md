# Проверка расширений: macOS и Windows

Дата: 2026-10-04. Tezbar: `main`, commit `88fab44`.

## Текущий результат после исправлений

Четыре воспроизведённые проблемы исправлены в исходниках:

- Tezbar сохраняет нативные байты/разделители `spawn.stdout`; Port Manager обнаруживает реально открытый тестовый порт.
- Добавлен `runPowerShellScript`: UTF-16LE encoded command, UTF-8 stdout, timeout/cancellation и безопасный record-mode.
- Google Translate выбирает `afplay` на macOS и STA MediaPlayer через PowerShell на Windows; download/playback errors возвращаются вызывающему коду, временные файлы изолированы и удаляются.
- VSCode CLI получает кавычки только на Windows, а не при нативном macOS `execFileSync`.
- `useSQL` использует встроенный read-only SQLite с закрытием соединения вместо внешнего `sqlite3.exe`.
- Исправлены две ошибочные предпосылки тестов: Windows-путь в POSIX path-тесте и отсутствие RS deinterleaving в тестовом QR-декодере. Production QR-генератор не изменялся.

Повторная проверка первого раунда: штатная suite Tezbar **342 passed / 1 Windows-only skipped**, portability **30/30**, Google Translate audio tests **6/6**, Raycast build/typecheck **7/7**, backend build и Tezbar typecheck успешно. Добавлены отдельные штатные регрессии для spawn, PowerShell и read-only SQLite.

Следующий раунд охватил ещё 8 установленных пакетов: [отчёт](./INSTALLED_EXTENSIONS_PLATFORM_AUDIT.md). Исправлены дополнительные API gaps и безопасный platform fallback; итоговая штатная suite — **351 passed / 1 Windows-only skipped**, installed probes — **21/21**. Read-only SQLite также проверен непосредственно под Bun на macOS.

**Полную Windows-совместимость пока нельзя объявить:** реального Windows-хоста и проверок аккаунтов нет. Ограничения non-premium Spotify и macOS-only команд намеренно остаются.

Проверки выполнялись на macOS ARM64, Node 26.7.0, pnpm 11.23.0. Подмена Windows-веток и моки не заменяют запуск приложения/установщика на Windows.

Ниже сохранены результаты **первоначального аудита до исправлений**, включая старые строки исходников и сообщения ошибок.

## Первоначально выполненные проверки

| Проверка                                                    | Результат                                        |
| ----------------------------------------------------------- | ------------------------------------------------ |
| `pnpm typecheck` в Tezbar                                   | Успешно                                          |
| Обычный `pnpm test`                                         | 331 passed, 2 failed, 1 skipped                  |
| `extension-runner.test.ts` внутри общей проверки            | 26 passed, 1 Windows-only skipped                |
| Google Calendar `npm test`                                  | 25/25 passed                                     |
| Kill Process `npm test`                                     | 4/4 passed                                       |
| Raycast `npm run build` всех расширений во временных копиях | 7/7 успешно                                      |
| `tsc --noEmit` после генерации Raycast-типов                | 7/7 успешно                                      |
| Сборка реальным builder Tezbar, выбор команд macOS          | 60/60 команд                                     |
| Сборка builder Tezbar, симулированный выбор Windows         | 58/58 совместимых команд                         |
| Дополнительные portability-пробы                            | 25 passed, 5 failed: четыре независимые проблемы |

Первоначальный запуск `tsc` без Raycast codegen выдавал ошибки отсутствующих `Preferences`/`Arguments`. После `ray build` все эти проверки прошли: это не платформенный дефект.

Открытие семи локальных представлений в backend runtime прошло: GIF favorites/recents, Translate/Translate Form, Kill Process, Named Ports/Open Ports. Однако более строгая проверка Open Ports с реально открытым тестовым TCP-сокетом выявила функциональную ошибку. Проверки UI означают построение runtime-дерева, а не ручное E2E-тестирование Tauri/WebView.

## Матрица расширений

| Расширение             | macOS: что проверено                                                                  | Windows: результат проверки кода/контрактов                                                                                                                          |
| ---------------------- | ------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GIF Search             | Сборка, пустые favorites/recents открываются                                          | Сборка проходит; реальные поиск/скачивание/clipboard не проверены. Квадратное копирование GIF намеренно скрыто на Windows                                            |
| Google Calendar        | Сборка, 25 unit-тестов                                                                | Сборка проходит; реальный Google OAuth и операции с аккаунтом не проверены                                                                                           |
| Google Translate       | Сборка, оба локальных представления открываются                                       | Озвучка вызывает отсутствующий на Windows `afplay`; перевод через сеть не проверен                                                                                   |
| Kill Process           | Сборка, 4 unit-теста, представление, реальный read-only `ps`                          | Генерация listing/restart/taskkill-команд проверена с моками; реальное выполнение PowerShell и завершение процессов не проверены                                     |
| Port Manager           | Нативный код обнаруживает тестовый порт, но runtime Tezbar показывает `No Open Ports` | Общая ошибка stdout также затрагивает строковый netstat-парсер; на Windows не запускалось. Menu-bar команда macOS-only                                               |
| Spotify Player         | Сборка; работа с аккаунтом/Spotify не проверена                                       | Для локального управления нужен отсутствующий runtime API `runPowerShellScript`; volume/position для non-premium явно не поддерживаются. Menu-bar команда macOS-only |
| VSCode Recent Projects | Сборка; реальный CLI-вызов падает с `ENOENT`                                          | Не хватает `runPowerShellScript` для Explorer/Command Palette. Чтение recent projects дополнительно требует внешнего `sqlite3.exe`                                   |

## Подтверждённые проблемы

### 1. Tezbar повреждает stdout `spawn`: Port Manager не видит порты

Файл: `src/main/extension-runner.ts:3783–3807`.

Перехватчик разбивает stdout на строки, применяет `trim()` и отдаёт слушателям `Buffer.from(trimmed)` **без разделителя строк**. Проба реального дочернего процесса воспроизвела:

```text
ожидалось: "first\nsecond\n"
получено:  "firstsecond"
```

Port Manager разбирает вывод netstat/lsof построчно. На macOS отдельно вызванный код расширения находит контролируемый TCP-сокет, а тот же Open Ports в Tezbar сообщает `No Open Ports`. Ошибка находится в общем runtime, поэтому Windows netstat-парсер тоже подвержен ей; Windows-выполнение пока не проверено.

### 2. Runtime не экспортирует `runPowerShellScript`

Файл: `src/main/extension-runner.ts`, `createRaycastUtilsShim`.

Минимальная команда в реальном Tezbar runtime вернула:

```text
Error: runPowerShellScript is missing from Tezbar runtime
```

API импортируется Spotify (`src/helpers/script.ts`) и VSCode (`src/utils/win-scripts.ts`, `src/index.tsx`). Поэтому соответствующие Windows-сценарии не заработают только от наличия PowerShell на машине.

### 3. Google Translate TTS использует `afplay` на Windows

Файл расширения: `../Tezbar Extensions/extensions/google-translate/src/simple-translate.ts:217`.

Проба реального `playTTS` с симулированным Windows, моками скачивания/файлов/процесса зафиксировала `spawn("afplay", ...)`. Условия по платформе нет. Это касается озвучки, а не всех функций перевода. На реальной Windows отсутствие `afplay` потребует отдельного воспроизведения.

### 4. VSCode CLI на macOS: кавычки являются частью executable path

Файл расширения: `../Tezbar Extensions/extensions/visual-studio-code-recent-projects/src/lib/vscode.ts:263`.

`this.cliFilename = \`"${cliFilename}"\``передаётся в`execFileSync`без shell на macOS. В результате ищется файл с буквальными кавычками в имени. Наличие настоящего`/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code`проверено; вызов через расширение воспроизвёл`ENOENT`. Затронуты CLI-операции открытия URI/установки/удаления VSCode extensions.

## Дополнительные ограничения и риски

- `useSQL` в `src/main/extension-runner.ts:2453` вызывает внешний `sqlite3.exe` на Windows. В просмотренном build/resource-конфиге его упаковки не найдено. VSCode recent projects использует `useSQL`; на чистой Windows потребуется CLI либо другой SQLite-адаптер. Это проверка зависимости по коду, не Windows E2E.
- Spotify Windows-ветка явно выбрасывает `WinNotSupportedError` для установки volume/position у non-premium пользователей. Наличие Premium/API-пути не проверялось с аккаунтом.
- macOS-only menu-bar команды Spotify и Port Manager корректно исключены из Windows-сборки: разница 60 против 58 ожидаема.
- Два падения обычной suite: `bun-manager.test.ts` использует Windows-путь при POSIX `path.dirname`, а QR round-trip для длинного текста возвращает `null`. Первое — непереносимая тестовая предпосылка; второе не связано с проверенными расширениями. Windows-only color picker тест был пропущен.
- Реальные OAuth, сетевой поиск GIF/перевод, clipboard/paste, изменение календарей, управление Spotify, убийство/перезапуск пользовательских процессов, установщик и Windows x64/ARM64 **не проверены**. Во время проб процессы пользователя не завершались, аккаунты не изменялись.

## Повторный запуск portability-проб

Из каталога `Tezbar`, при наличии соседнего checkout `../Tezbar Extensions/extensions` с установленными зависимостями:

```sh
pnpm exec vitest run --config tests/manual/extension-portability.vitest.config.ts
```

Harness рассчитан на текущую macOS-среду, включает симуляцию Windows-веток и read-only проверки macOS. Теперь ожидается **30 passed**, exit code 0. Первоначально было **5 failed / 25 passed**; два падения относились к одному дефекту stdout. Пробы выполняют сборки во временных копиях; исходные расширения не изменяют. Они исключены из обычного `pnpm test`.

Артефакты текущего запуска:

- `/tmp/tezbar-portability-probes.log`
- `/tmp/tezbar-extension-cli-validation.log`
- `/tmp/tezbar-extensions-unit-tests.log`
- `/tmp/tezbar-extension-host-tests.log`

Перед заявлением полной поддержки Windows нужны исправления подтверждённых проблем, затем запуск приложения на настоящих Windows x64/ARM64 с реальными аккаунтами и проверка основных действий каждого расширения.
