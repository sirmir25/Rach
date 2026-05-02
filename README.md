# Rach

> Пиши просто — запускай везде.

Rach — маленький скриптовый язык с фокусом на автоматизацию: системные команды, файлы, браузер (через настоящий W3C WebDriver), генерация bash и кода на других языках. Интерпретатор написан на Rust, один статически слинкованный бинарь, без рантайм-зависимостей кроме `curl`/`tar`/`unzip` для авто-установки веб-драйверов.

---

## Содержание

- [Установка](#установка)
- [Hello, world](#hello-world)
- [Структура скрипта](#структура-скрипта)
- [Модули (import)](#модули-import)
- [Команды стандартной библиотеки](#команды-стандартной-библиотеки)
  - [os / system](#os--system)
  - [Файлы](#файлы)
  - [run_command / install_package](#run_command--install_package)
  - [Браузер (WebDriver)](#браузер-webdriver)
  - [bash DSL](#bash-dsl)
  - [ai_generate](#ai_generate)
- [Управление потоком: `if linux/macos/windows`](#управление-потоком-if-linuxmacoswindows)
- [Соглашение об ошибках](#соглашение-об-ошибках)
- [Переменные окружения](#переменные-окружения)
- [Сборка из исходников](#сборка-из-исходников)
- [CLI](#cli)
- [Грамматика (формально)](#грамматика-формально)
- [Ограничения и не-цели](#ограничения-и-не-цели)
- [Лицензия](#лицензия)

---

## Установка

Нужен Rust 1.70+ и `cargo`. Сборка:

```bash
git clone https://github.com/<USER>/rach.git
cd rach
cargo build --release
sudo ln -s "$PWD/target/release/rach" /usr/local/bin/rach
rach version
```

Для автоматизации браузера также нужен один из:
- Chrome или Chromium (тогда `chromedriver` скачается автоматически)
- Firefox (тогда `geckodriver` скачается автоматически)
- Microsoft Edge с уже установленным `msedgedriver`
- Safari + `safaridriver --enable` + галка "Allow Remote Automation" в Develop меню

`curl`, `tar`, `unzip` должны быть в PATH (на macOS/Linux есть из коробки).

---

## Hello, world

`hello.rach`:

```
import os
import system

rach main(0)
    detect os()
    create_file("/tmp/hello.txt", "привет от Rach")
    read_file("/tmp/hello.txt")
    completed
return(end)
(end0)
```

Запуск:

```bash
rach hello.rach
```

Вывод:

```
os: macos
completed
created: /tmp/hello.txt
completed
привет от Rach
completed
completed
```

---

## Структура скрипта

Каждый файл `.rach` — это:

```
import <модуль1>
import <модуль2>
...

rach <имя>(<арность>)
    <команды>
return(end)
(end<N>)
```

Правила:

- В файле должна быть функция с именем `main` — с неё начинается выполнение.
- `<арность>` — целое число; пока всегда `0` (аргументы функций ещё не реализованы, но синтаксис уже поддерживает).
- `return(end)` отмечает конец тела функции, `(end0)` — конец файла. `(end0)`, `(end1)` и т.п. эквивалентны: суффиксная цифра не несёт смысла, она просто часть синтаксической метки.
- Комментарии: `#` или `//` до конца строки.
- Отступы значимы только внутри блоков `if linux/macos/windows`.

---

## Модули (import)

Импорты — декларативные. Они не загружают кода (стандартная библиотека всегда вкомпилирована в интерпретатор), но служат документацией намерений. Неизвестные модули вызывают warning, но не ошибку.

| Модуль       | Что объявляет                                     |
|--------------|---------------------------------------------------|
| `os`         | `detect_os`, проверки `if linux/macos/windows`    |
| `system`     | файлы, `run_command`, `install_package`, `reboot` |
| `web`        | автоматизация браузера                            |
| `browser`    | синоним для `web` (alias по смыслу)               |
| `linux`      | OS-специфичный namespace                          |
| `windows`    | OS-специфичный namespace                          |
| `macos`      | OS-специфичный namespace                          |
| `bash`       | DSL `bash = generate ...`                         |
| `ai`         | `ai_generate(...)`                                |

---

## Команды стандартной библиотеки

Команды в Rach пишутся "по-английски": несколько слов подряд образуют имя команды, скобки — аргументы. Пример:

```
open in browser("https://example.com")   // → open_in_browser("https://...")
fill form id("login") value("ilia")      // → fill_form, kwargs id=..., value=...
wait seconds(3)                          // → wait_seconds(3)
```

Имя команды разрешается интерпретатором: ищется самый длинный префикс из слов, совпадающий с известной командой. Остальные слова + их `(...)` становятся keyword-аргументами.

### os / system

| Команда                  | Что делает                                                      |
|--------------------------|------------------------------------------------------------------|
| `detect os()`            | Печатает текущую ОС: `linux`, `macos`, `windows`, `bsd`         |
| `reboot()`               | Печатает намерение перезагрузки (без выполнения — безопасность) |
| `shutdown()`             | Аналогично, без выполнения                                      |

### Файлы

| Команда                                     | Эффект                                |
|---------------------------------------------|----------------------------------------|
| `create_file("/path", "содержимое")`        | Создаёт файл, перезаписывает если есть |
| `read_file("/path")`                        | Печатает содержимое                    |
| `edit_file("/path", "новое содержимое")`    | Перезаписывает                         |
| `delete_file("/path")`                      | Удаляет                                |
| `check_if_exists("/path")`                  | Печатает `exists` или `missing`        |

### run_command / install_package

```
run_command("ls -la /tmp")
install_package("htop")
```

`run_command` запускает команду через `sh -c` (на Windows — `cmd /C`) и печатает stdout/stderr.

`install_package` подбирает менеджер пакетов под ОС:

| ОС      | Команда                                       |
|---------|-----------------------------------------------|
| macOS   | `brew install <pkg>`                          |
| Linux   | `apt-get` / `dnf` / `pacman` / `zypper` / `apk` (под sudo) |
| Windows | `winget install --silent <pkg>`               |
| BSD     | `pkg install -y <pkg>`                        |

Установка реально выполняется. Чтобы запустить только в режиме "что было бы сделано":

```bash
RACH_DRY_RUN=1 rach install.rach
```

### Браузер (WebDriver)

Все команды браузера ходят через настоящий [W3C WebDriver](https://www.w3.org/TR/webdriver2/) (HTTP-протокол). Драйвер запускается автоматически при первой браузерной команде:

1. Если `chromedriver`/`geckodriver`/`msedgedriver` есть в PATH — используется он.
2. Иначе ищется в кэше `~/Library/Caches/rach/drivers/` (на Linux — `~/.cache/rach/drivers/`).
3. Иначе скачивается:
   - `chromedriver` — через [Chrome for Testing API](https://googlechromelabs.github.io/chrome-for-testing/), если установлен Chrome/Chromium.
   - `geckodriver` v0.36.0 с GitHub Releases, если установлен Firefox.

Список команд:

| Команда                                              | Что делает                                          |
|------------------------------------------------------|------------------------------------------------------|
| `open in browser("url")`                             | Запустить любой доступный браузер и открыть URL     |
| `open in chrome("url")` / `firefox` / `edge` / `safari` | Принудительно конкретный браузер                |
| `navigate to("url")`                                 | Перейти на URL в текущей вкладке                    |
| `open new tab("url")`                                | Новая вкладка                                       |
| `switch tab(2)`                                      | Переключиться на вкладку #N (1-индексация)          |
| `wait seconds(N)`                                    | Подождать (макс. 600)                               |
| `scroll down pixels(600)`                            | Прокрутить через `window.scrollBy`                  |
| `take screenshot("/tmp/x.png")`                      | Скриншот через WebDriver, PNG                       |
| `press key("Enter")`                                 | Послать спец-клавишу активному элементу             |
| `click button("Sign in")`                            | Найти кнопку по тексту и кликнуть                   |
| `click element("#submit")` / `(".cls")` / `("//xpath")` | Кликнуть по селектору                            |
| `type text("input_id", "текст")`                     | Напечатать в элемент                                |
| `fill form id("login") value("ivan")`                | Найти по id/name, очистить, ввести                  |
| `login user("ivan") pws("secret")`                   | Найти типичные поля login+password, нажать Enter    |
| `execute js("return document.title")`                | Выполнить JS, напечатать результат                  |
| `download file("url", "/path")`                      | Скачать через `curl -L`                             |
| `upload file("/local/path", "input_id")`             | Через `send_keys` в `<input type=file>`             |

Поддерживаемые имена клавиш для `press key`: `Enter/Return`, `Tab`, `Escape/Esc`, `Space`, `Backspace`, `Delete`, `Up/Down/Left/Right` (с/без `Arrow_` префикса), `Home`, `End`, `PageUp`, `PageDown`.

Стратегии селекторов в `click_element`/`type_text`/`fill_form`:

- начинается с `/` — XPath
- начинается с `#`, `.`, или содержит пробел/`[` — CSS selector
- иначе — `[id='X'],[name='X']`

Headless-режим:

```bash
RACH_HEADLESS=1 rach script.rach
```

(Поддерживается для Chrome/Edge/Firefox; Safari headless не умеет.)

### bash DSL

Внутри тела `main` можно встретить присваивания вида `<что_угодно> = <действие> <текст>`:

```
bash = generate install oh my zsh           # Сгенерирует один-лайнер
bash = search curl or wget                  # Кратко "что есть в системе для X"
bash = web search site ohmyzsh              # Лог намерения веб-поиска (запросов не шлёт)
bash = complete or error                    # Просто печатает `completed`
```

Это намеренно слабый, эвристический DSL — для коротких заметок и подсказок. Для реального исполнения bash используй `run_command("...")`.

Распознаваемые задачи в `generate`: oh-my-zsh, homebrew, curl/wget, обновление apt, диск/память/CPU. Остальное даёт `# TODO: ...`-стаб.

### ai_generate

Встроенный шаблонный генератор кода. Не ходит в сеть, не вызывает LLM — это библиотека готовых сниппетов для частых задач.

```
ai_generate(language="bash", task="установить oh-my-zsh на Linux")
ai_generate(language="rust", task="простой TCP сервер")
ai_generate(language="python", task="простой парсер JSON")
```

Поддерживаемые языки: `bash` (alias `sh`), `python` (`py`), `rust` (`rs`), `c++` (`cpp`/`cxx`), `c`, `zig`.

Распознаваемые задачи:
- `bash`: oh-my-zsh, обновление пакетов
- `python`: TCP-сервер, парсинг JSON
- `rust`: TCP-сервер, копирование файлов
- `c++`/`c`: копирование файлов
- `zig`: парсинг JSON

Для всего остального возвращает корректный TODO-стаб.

---

## Управление потоком: `if linux/macos/windows`

Единственный условный оператор — проверка ОС:

```
rach main(0)
    detect os()
    if linux:
        run_command("apt-get update")
    if macos:
        run_command("brew update")
    if windows:
        run_command("winget upgrade --all")
    completed
return(end)
(end0)
```

Тело блока — все строки с отступом больше, чем у `if`. Пустых блоков `else` нет — пиши несколько отдельных `if`.

`macos` синонимичен `darwin`.

---

## Соглашение об ошибках

Каждая команда после успеха печатает `completed`. После неуспеха — строку вида:

```
error <код> string <номер_строки>  // <пояснение>
```

Коды близки по смыслу к HTTP:

| Код  | Значение                                        |
|------|--------------------------------------------------|
| 400  | Плохой ввод (некорректные аргументы)            |
| 404  | Не найдено (файл, команда, элемент DOM)         |
| 409  | Конфликт состояния (нет активной браузерной сессии) |
| 422  | Синтаксическая ошибка парсера                   |
| 500  | Внутренняя ошибка (I/O, спавн процесса)         |
| 501  | Не реализовано на этой ОС                       |
| 502  | Подсистема упала (внешний драйвер, сеть)        |
| 503  | Сервис недоступен (не удалось поднять WebDriver)|

Можно вручную поднять ошибку:

```
error 409 string 12
```

— это просто печатает строку (не прерывает выполнение).

---

## Переменные окружения

| Переменная             | Что делает                                                  |
|------------------------|-------------------------------------------------------------|
| `RACH_HEADLESS`        | `1` — запустить браузер в headless-режиме                  |
| `RACH_DRY_RUN`         | `1` — `install_package` только печатает команду, не запускает |
| `RACH_DRIVER_DIR`      | Папка для кэша скачанных WebDriver-бинарников               |

---

## Сборка из исходников

```bash
cargo build --release
./target/release/rach examples/hello.rach
```

Зависимости: только `serde_json`. Всё остальное — `std`.

Кросс-компиляция:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

Получаешь статически слинкованный бинарь, который запустится на любом Linux без glibc-вопросов.

---

## CLI

```
rach <file.rach>          запустить скрипт
rach run <file.rach>      то же самое
rach check <file.rach>    только проверить синтаксис, без выполнения
rach version              вывести версию
rach help                 краткая справка
```

Коды выхода:

| Код | Когда                                |
|-----|---------------------------------------|
| 0   | Успешно                              |
| 1   | Ошибка времени выполнения            |
| 2   | Не удалось прочитать файл            |
| 3   | Лексическая ошибка                   |
| 4   | Ошибка парсинга                      |

---

## Грамматика (формально)

```
program       := { import_line } { function }
import_line   := "import" IDENT NEWLINE
function      := "rach" IDENT "(" INT ")" NEWLINE
                   { stmt }
                 "return" "(" "end" ")" NEWLINE
                 "(" "end" [ INT ] ")" NEWLINE

stmt          := if_stmt
              | bash_dsl
              | ai_generate_call
              | "completed" NEWLINE
              | "error" INT [ "string" INT ] NEWLINE
              | call

if_stmt       := "if" IDENT ":" NEWLINE
                   { stmt at indent > if's-column }

bash_dsl      := IDENT "=" rest-of-line NEWLINE
ai_generate_call := "ai_generate" "(" kw_args ")" NEWLINE

call          := segment { segment } NEWLINE
segment       := IDENT { IDENT } "(" arg_list ")"
arg_list      := arg { "," arg }
arg           := STRING | INT | IDENT | IDENT "=" (STRING | INT | IDENT)
```

Лексер рассматривает `\n` как значимый разделитель. Идентификаторы — `[A-Za-z_][A-Za-z0-9_]*`. Строки — двойные кавычки с поддержкой `\\`, `\"`, `\n`, `\t`, `\r`. Числа — целые, со знаком.

---

## Ограничения и не-цели

Сейчас в Rach **нет**:

- Переменных и присваиваний (кроме `bash =` DSL — но это не настоящая переменная).
- Арифметики, строк-операций, сравнений.
- Циклов и пользовательских условий (только `if linux/macos/windows`).
- Пользовательских функций кроме `main`.
- Импорта собственных файлов.
- Реальных аргументов функций.
- Try/catch — ошибки печатаются, не прерывают выполнение.
- LLM в `ai_generate` — это шаблоны.

Это намеренно: язык задуман как декларативный скрипт-DSL для автоматизации, не как general-purpose. Если нужна логика — пиши `run_command("python3 -c '...'")` или `ai_generate(language="python", task="...")` и пусть Python делает работу.

---

## Лицензия

TBD. Рекомендую MIT или Apache-2.0. Без `LICENSE` в репозитории код юридически не переиспользуем.
