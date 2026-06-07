import { createContext, useContext, useEffect, type ReactNode } from "react";

type Language = "en" | "ru";

const dictionary: Record<string, string> = {
  Overview: "Обзор",
  Hosts: "Хосты",
  Services: "Сервисы",
  Database: "Базы данных",
  CMS: "CMS",
  Logs: "Логи",
  "System Status": "Статус системы",
  "All services running": "Все сервисы запущены",
  "All services stopped": "Все сервисы остановлены",
  "services running": "сервисов запущено",
  Memory: "Память",
  Disk: "Диск",
  Uptime: "Аптайм",
  "Start All": "Запустить все",
  Stop: "Остановить",
  Restart: "Перезапустить",
  "New Host": "Новый хост",
  Import: "Импорт",
  Export: "Экспорт",
  Duplicate: "Дублировать",
  Delete: "Удалить",
  Filters: "Фильтры",
  "More host actions": "Еще действия с хостами",
  "Export Hosts": "Экспорт хостов",
  "Open Hosts Folder": "Открыть папку hosts",
  "Open filter presets": "Открыть пресеты фильтров",
  "Reset Filters": "Сбросить фильтры",
  "Running Hosts": "Запущенные хосты",
  "Needs Attention": "Требуют внимания",
  Pin: "Закрепить",
  Unpin: "Открепить",
  "Compact Rows": "Компактные строки",
  "Comfortable Rows": "Обычные строки",
  "Action Center": "Центр действий",
  "Recent actions": "Последние действия",
  active: "активно",
  error: "ошибка",
  Fix: "Исправить",
  "Action failed": "Действие не выполнено",
  "Hosts file issue": "Проблема hosts-файла",
  "Missing dependency": "Отсутствует зависимость",
  "Service startup issue": "Проблема запуска сервиса",
  "Sync the Windows hosts file and approve administrator access.": "Синхронизируйте Windows hosts-файл и подтвердите права администратора.",
  "Install missing service files or detect installed tools.": "Установите отсутствующие файлы сервисов или выполните поиск установленных инструментов.",
  "Run automatic repair to refresh configs, ports and service state.": "Запустите автоматическое исправление конфигов, портов и состояния сервисов.",
  "Check the details, then retry the action or open Logs.": "Проверьте детали, затем повторите действие или откройте логи.",
  "Autostart services stopped": "Автозапускаемые сервисы остановлены",
  "Hosts need attention": "Хосты требуют внимания",
  "SSL trust checks": "Проверки доверия SSL",
  "Port Manager": "Менеджер портов",
  "Scan Ports": "Сканировать порты",
  "Click Scan Ports to inspect local service ports.": "Нажмите Сканировать порты, чтобы проверить локальные порты сервисов.",
  "Notification Level": "Уровень уведомлений",
  "Errors only": "Только ошибки",
  "Errors and completed actions": "Ошибки и завершенные действия",
  "All events": "Все события",
  "Scheduled Backups": "Резервные копии по расписанию",
  "Backup Databases": "Резервные копии баз",
  "Scheduled backups are enabled for the local reminder panel.": "Резервные копии по расписанию включены для локальной панели напоминаний.",
  "Scheduled backups are paused.": "Резервные копии по расписанию приостановлены.",
  Performance: "Производительность",
  "Health Score": "Оценка здоровья",
  "Environment Preset": "Профиль окружения",
  "Git Import": "Импорт из Git",
  "Repository URL": "URL репозитория",
  "Import from Git": "Импортировать из Git",
  Project: "Проект",
  "Open Installed Site": "Открыть установленный сайт",
  "npm install": "npm install",
  "npm run dev": "npm run dev",
  "composer install": "composer install",
  "artisan migrate": "artisan migrate",
  "First Run Wizard": "Мастер первого запуска",
  "Choose project folders, check ports, trust SSL, and start base services.": "Выберите папки проектов, проверьте порты, доверие SSL и запустите базовые сервисы.",
  "Open Settings": "Открыть настройки",
  "Prepare Environment": "Подготовить окружение",
  Done: "Готово",
  "Update available": "Доступно обновление",
  "You are up to date": "Установлена последняя версия",
  "Open Release": "Открыть релиз",
  "Detect Project": "Определить проект",
  "Generate .env": "Создать .env",
  "Credential Vault": "Хранилище доступов",
  "Select saved credentials": "Выбрать сохраненные доступы",
  "Vault Name": "Имя записи",
  "Vault Tool": "Хранилище",
  "Save credentials": "Сохранить доступы",
  "Preview Site": "Проверить сайт",
  "Export Portable": "Экспорт portable",
  General: "Общие",
  Paths: "Пути",
  Startup: "Автозапуск",
  Network: "Сеть",
  Theme: "Тема",
  Light: "Светлая",
  Dark: "Темная",
  Pearl: "Жемчужная",
  Graphite: "Графит",
  Azure: "Лазурная",
  Forest: "Лесная",
  Midnight: "Полуночная",
  Carbon: "Карбон",
  Blue: "Синяя",
  Green: "Зеленая",
  Slate: "Сланцевая",
  "High Contrast": "Высокий контраст",
  System: "Системная",
  Notifications: "Уведомления",
  Integrations: "Интеграции",
  Updates: "Обновления",
  Backups: "Резервные копии",
  Advanced: "Дополнительно",
  Application: "Приложение",
  Behavior: "Поведение",
  Language: "Язык",
  "Preferred Browser": "Предпочитаемый браузер",
  "UI Density": "Плотность интерфейса",
  "Minimize to System Tray": "Сворачивать в системный трей",
  "Close to System Tray": "Закрывать в системный трей",
  "Enable Telemetry": "Включить телеметрию",
  "Projects Folder": "Папка проектов",
  "Services Folder": "Папка сервисов",
  "Backups Folder": "Папка резервных копий",
  "Open Projects": "Открыть проекты",
  "Open Services": "Открыть сервисы",
  "Open Backups": "Открыть резервные копии",
  "Launch on Startup": "Запускать вместе с Windows",
  "Start Minimized to Tray": "Запускать свернутым в трей",
  "HTTP Port Start": "Начальный HTTP-порт",
  "HTTP Port End": "Конечный HTTP-порт",
  "Proxy Enabled": "Прокси включен",
  "Show Notifications": "Показывать уведомления",
  "Play Sound on Events": "Звук при событиях",
  "Open App Data": "Открыть AppData",
  "Check for Updates on Startup": "Проверять обновления при запуске",
  "Check Now": "Проверить сейчас",
  "Backup Retention Days": "Хранить резервные копии, дней",
  "Create Backup": "Создать резервную копию",
  "Restore Backup": "Восстановить резервную копию",
  Logging: "Логирование",
  "Log Level": "Уровень логов",
  "Max Log File Size": "Максимальный размер лог-файла",
  "Retain Logs Days": "Хранить логи, дней",
  "Show Timestamps": "Показывать время",
  "Reset All Warnings": "Сбросить предупреждения",
  "Import / Export Settings": "Импорт / экспорт настроек",
  "Export Settings": "Экспорт настроек",
  "Import Settings": "Импорт настроек",
  "Save Settings": "Сохранить настройки",
  "Reset Settings": "Сброс настроек",
  "Reset to Defaults": "Сбросить по умолчанию",
  "About Settings": "О настройках",
  "Need Help?": "Нужна помощь?",
  "Visit documentation for detailed guides and troubleshooting.": "Откройте локальную папку документации и данных приложения.",
  "Open Documentation": "Открыть документацию",
  "Default System Browser": "Браузер по умолчанию",
  Chrome: "Chrome",
  Edge: "Edge",
  Firefox: "Firefox",
  Comfortable: "Комфортная",
  Compact: "Компактная",
  "English (US)": "Английский (США)",
  Russian: "Русский",
  Settings: "Настройки",
  Apache: "Apache",
  Nginx: "Nginx",
  MySQL: "MySQL",
  Redis: "Redis",
  Information: "Информация",
  Warning: "Предупреждение",
  Error: "Ошибка",
  Debug: "Отладка",
  "10 MB": "10 МБ",
  "50 MB": "50 МБ",
  "100 MB": "100 МБ",
  "Configure the core behavior and preferences of the application.": "Настройка основного поведения и предпочтений приложения.",
  "Manage default folders for projects, logs, and services.": "Управление папками проектов, логов и сервисов.",
  "Configure what happens when LocalStack Pro starts.": "Настройка поведения при запуске LocalStack Pro.",
  "Manage ports, proxies, and network resolution.": "Управление портами, прокси и сетевым разрешением.",
  "Customize the appearance of the application.": "Настройка внешнего вида приложения.",
  "Control how and when you receive alerts.": "Управление уведомлениями и звуковыми событиями.",
  "Connect LocalStack Pro with external tools.": "Интеграция LocalStack Pro с внешними инструментами.",
  "Set your update channel and update behavior.": "Настройка проверки обновлений.",
  "Configure automatic backups and retention.": "Настройка резервных копий и срока хранения.",
  "Advanced settings for power users.": "Расширенные настройки для опытных пользователей.",
  "Action completed.": "Действие выполнено.",
  "Action in progress...": "Выполняется действие...",
  Operations: "Операции",
  Running: "Выполняется",
  success: "Готово",
  "Open in Browser": "Открыть в браузере",
  "Open Root Folder": "Открыть корневую папку",
  "View Logs": "Смотреть логи",
  "Tail File": "Читать файл",
  "Reading log file": "Чтение log-файла",
  "Running health check...": "Проверка окружения...",
  "Running final health check...": "Финальная проверка окружения...",
  "Detecting installed dependencies...": "Поиск установленных зависимостей...",
  "Repair All": "Исправить все",
  "One-click Fix": "Исправить в один клик",
  "Startup Doctor": "Стартовая диагностика",
  "Service Profiles": "Профили сервисов",
  "Security Center": "Центр безопасности",
  "Smart Logs": "Умные логи",
  "Open service ports": "Открытые порты сервисов",
  "Weak database passwords": "Слабые пароли баз данных",
  "Untrusted certificates": "Недоверенные сертификаты",
  "PHP display_errors": "PHP display_errors",
  "No recurring issues detected.": "Повторяющиеся проблемы не найдены.",
  "Readiness": "Готовность",
  Ready: "Готов",
  "Needs attention": "Требует внимания",
  "Not ready": "Не готов",
  "Repair Host": "Исправить хост",
  "Repairing environment...": "Исправление окружения...",
  "Refreshing CMS templates...": "Обновление шаблонов CMS...",
  "Installing PHP 8.4...": "Установка PHP 8.4...",
  "Refreshing PHP versions...": "Обновление версий PHP...",
  "File Tail": "Хвост файла",
  "Log Details": "Детали лога",
  Statistics: "Статистика",
  "SSL Certificate": "SSL-сертификат",
  "Create Database": "Создать базу",
  "Import SQL": "Импорт SQL",
  "Open phpMyAdmin": "Открыть phpMyAdmin",
  "Open Adminer": "Открыть Adminer",
  "Backup All": "Создать резервную копию",
  "Popular CMS": "Популярные CMS",
  "Installed CMS": "Установленные CMS",
  Installation: "Установка",
  "Install CMS": "Установить CMS",
  "Download and Install": "Скачать и установить",
  "Install Flow": "Процесс установки",
  "Create database": "Создать базу данных",
  "Overwrite existing files": "Перезаписать существующие файлы",
  "Project Folder": "Папка проекта",
  "Open Folder": "Открыть папку",
  Source: "Источник",
  "Sync Hosts File": "Синхронизировать hosts",
  "Official package": "Официальный пакет",
  "No CMS installations yet.": "CMS пока не установлены.",
  "Generate Certificate": "Создать сертификат",
  "Trust Certificate": "Доверять сертификату",
  Revoke: "Отозвать",
  "Open Certificate Store": "Открыть хранилище сертификатов",
  "Repair Trust": "Исправить доверие",
  Summary: "Итог",
  OK: "OK",
  Warnings: "Предупреждения",
  Errors: "Ошибки",
  Critical: "Критичные",
  "Total checks": "Всего проверок",
  "All checks passed.": "Все проверки пройдены.",
  Start: "Запустить",
  "Stop All": "Остановить все",
  "Restart All": "Перезапустить все",
  Detect: "Найти",
  Install: "Установить",
  Config: "Конфиг",
  Autostart: "Автозапуск",
  Version: "Версия",
  Executable: "Файл запуска",
  Log: "Лог",
  Ports: "Порты",
  "Process ID": "PID процесса",
  "Edit Config": "Редактировать конфиг",
  "Open Main Window": "Открыть главное окно",
  "Active Hosts": "Активные хосты",
  "Recent Activity": "Последняя активность",
  "Quick Actions": "Быстрые действия",
  "Quit LocalStack Pro": "Выйти из LocalStack Pro",
  "Open shop.test": "Открыть shop.test",
  "Open Site": "Открыть сайт",
  "Open PHP page and install a compatible PHP version first.": "Откройте раздел PHP и установите совместимую версию PHP.",
  Domain: "Домен",
  Host: "Хост",
  "Root Folder": "Корневая папка",
  Folder: "Папка",
  "PHP Version": "Версия PHP",
  "Web Server": "Веб-сервер",
  "Document Root": "Document Root",
  "Error Log": "Лог ошибок",
  Status: "Статус",
  Environment: "Окружение",
  Tags: "Теги",
  Updated: "Обновлено",
  Actions: "Действия",
  Disabled: "Отключено",
  Valid: "Действителен",
  Stopped: "Остановлен",
  Starting: "Запускается",
  "All Status": "Все статусы",
  "All Environments": "Все окружения",
  "All PHP Versions": "Все версии PHP",
  "SSL: All": "SSL: все",
  "SSL: Enabled": "SSL: включен",
  "SSL: Disabled": "SSL: отключен",
  "Search hosts...": "Поиск хостов...",
  "Open in Terminal": "Открыть в терминале",
  "Host Diagnostics": "Диагностика хоста",
  Checks: "Проверки",
  Issues: "Проблемы",
  Healthy: "Исправно",
  Notes: "Заметки",
  Edit: "Изменить",
  Diagnose: "Диагностика",
  "Manage and monitor all system services": "Управление и мониторинг всех системных сервисов",
  "Install Missing": "Установить отсутствующие",
  "Missing dependencies installed or detected.": "Отсутствующие зависимости установлены или найдены.",
  "PHP-only": "Только PHP",
  "Install Version": "Установить версию",
  "PHP Versions": "Версии PHP",
  Default: "По умолчанию",
  "CLI Path": "Путь CLI",
  "SAPI Mode": "Режим SAPI",
  Extensions: "Расширения",
  Compatibility: "Совместимость",
  Active: "Активна",
  Installed: "Установлена",
  "PHP Actions": "Действия PHP",
  "Edit php.ini": "Редактировать php.ini",
  "Open CLI": "Открыть CLI",
  "Switch Default": "Сделать основной",
  "Remove Version": "Удалить версию",
  "Save Changes": "Сохранить изменения",
  Databases: "Базы данных",
  Engine: "Движок",
  Schemas: "Схемы",
  User: "Пользователь",
  Username: "Имя пользователя",
  Password: "Пароль",
  Port: "Порт",
  Size: "Размер",
  "Test Connection": "Проверить подключение",
  "Database Usage": "Использование базы",
  Total: "Всего",
  "Recent Activity / Query Log": "Последняя активность / журнал запросов",
  "Connection String": "Строка подключения",
  "Connection Diagnostics": "Диагностика подключения",
  "Recent Backups": "Последние резервные копии",
  Name: "Название",
  "Refresh Templates": "Обновить шаблоны",
  "Database Engine": "Движок базы",
  "Admin User": "Администратор",
  "Admin Password": "Пароль администратора",
  "Site Title": "Название сайта",
  "SSL Enabled": "SSL включен",
  "Services Ready": "Сервисы готовы",
  "Templates": "Шаблоны",
  Certificates: "Сертификаты",
  Issuer: "Издатель",
  Expires: "Истекает",
  Trust: "Доверие",
  "SAN Domains": "SAN-домены",
  "Certificate Details": "Детали сертификата",
  "Export Certificate": "Экспорт сертификата",
  "Trust Status": "Статус доверия",
  "Local CA": "Локальный CA",
  "Live Tail": "Live tail",
  Pause: "Пауза",
  Clear: "Очистить",
  Level: "Уровень",
  Service: "Сервис",
  Time: "Время",
  Message: "Сообщение",
  Details: "Детали",
  "Search logs...": "Поиск логов...",
  "All Services": "Все сервисы",
  "All Levels": "Все уровни",
  "All Hosts": "Все хосты",
  errors: "ошибок",
  warnings: "предупреждений",
  More: "Еще",
  Open: "Открыть",
  "View Full Logs": "Открыть все логи",
  Site: "Сайт"
};

const I18nContext = createContext<Language>("en");

export function languageFromSetting(value: string): Language {
  return value.toLowerCase().includes("russian") || value.toLowerCase().includes("рус") ? "ru" : "en";
}

export function I18nProvider({ language, children }: { language: string; children: ReactNode }) {
  const activeLanguage = languageFromSetting(language);
  useEffect(() => {
    if (activeLanguage !== "ru" || typeof document === "undefined") return;
    const frame = window.requestAnimationFrame(() => translateDom(document.body));
    return () => window.cancelAnimationFrame(frame);
  }, [activeLanguage]);
  return <I18nContext.Provider value={activeLanguage}>{children}</I18nContext.Provider>;
}

export function translate(text: string, language: Language) {
  if (language !== "ru") return text;
  if (dictionary[text]) return dictionary[text];
  if (dictionary[text.trim()]) return dictionary[text.trim()];
  if (/^\d+\s+hosts$/.test(text)) return text.replace("hosts", "хостов");
  if (/^\d+\s+services running$/.test(text)) return text.replace("services running", "сервисов запущено");
  if (/^\d+\s+Running$/.test(text)) return text.replace("Running", "запущено");
  if (/^\d+m ago$/.test(text)) return text.replace("m ago", "мин назад");
  if (/^Rows:\s+\d+\s+of\s+\d+$/.test(text)) return text.replace("Rows:", "Строк:").replace("of", "из");
  if (/^\d+\s+selected$/.test(text)) return text.replace("selected", "выбрано");
  if (/^\d+\s+errors\s+\/\s+\d+\s+warnings$/.test(text)) return text.replace("errors", "ошибок").replace("warnings", "предупреждений");
  if (/^\d+\s+OK\s+\/\s+\d+\s+Warning\s+\/\s+\d+\s+Error$/.test(text)) return text.replace("Warning", "предупреждений").replace("Error", "ошибок");
  if (text.startsWith("Installing ") || text.startsWith("Download")) return "Установка и подготовка...";
  if (text.endsWith(" installed or detected.")) return "Компонент установлен или найден.";
  if (text.startsWith("Opening ")) return "Открытие...";
  if (text.startsWith("Open ")) return `Открыть ${text.slice(5)}`;
  if (text.startsWith("Starting all")) return "Запуск всех сервисов...";
  if (text.startsWith("Stopping all")) return "Остановка всех сервисов...";
  if (text.startsWith("Restarting all")) return "Перезапуск всех сервисов...";
  if (text.startsWith("Starting ") || text.startsWith("Stopping ") || text.startsWith("Restarting ")) return "Операция с сервисом...";
  if (text.startsWith("Detecting ")) return "Поиск установленных зависимостей...";
  if (text.startsWith("Saving ")) return "Сохранение...";
  if (text.startsWith("Synchronizing ")) return "Синхронизация Windows hosts...";
  if (text.startsWith("Creating ")) return "Создание базы данных...";
  if (text.startsWith("Importing SQL")) return "Импорт SQL...";
  if (text.startsWith("Exporting ")) return "Экспорт...";
  if (text.startsWith("Deleting ")) return "Удаление...";
  if (text.startsWith("Creating application backup")) return "Создание резервной копии...";
  if (text.startsWith("Restoring application backup")) return "Восстановление резервной копии...";
  if (text.startsWith("Generating certificate")) return "Создание SSL-сертификата...";
  if (text.startsWith("Trusting certificate") || text.startsWith("Repairing trust")) return "Настройка доверия SSL...";
  if (text.startsWith("Revoking certificate")) return "Отзыв SSL-сертификата...";
  if (text.includes("was not found or installed")) return "Компонент не найден и не установлен. Запустите установку из приложения или укажите корректный путь.";
  if (text.includes("Executable not found")) return "Файл запуска не найден. Установите сервис или укажите корректный путь.";
  if (text.includes("Permission denied")) return "Нет прав доступа. Проверьте права на папку и запустите действие повторно.";
  if (text.includes("already exists")) return "Такой объект уже существует. Выберите другое имя или включите перезапись.";
  if (text.includes("not mapped in the Windows hosts file")) return "Домен не добавлен в Windows hosts. Нажмите синхронизацию hosts-файла.";
  if (text.includes("did not answer on port")) return "Сайт не отвечает на порту. Синхронизируйте hosts-файл и перезапустите веб-сервер.";
  if (text.includes("Cannot create CMS database")) return "Не удалось создать базу CMS. Запустите MySQL и проверьте учетные данные.";
  if (text.includes("Cannot open")) return "Не удалось открыть файл, папку или ссылку.";
  if (text.includes("Cannot start")) return "Не удалось запустить сервис.";
  if (text.includes("Cannot stop")) return "Не удалось остановить сервис.";
  if (text.includes("Cannot install")) return "Не удалось установить компонент.";
  if (text.includes("Cannot download")) return "Не удалось скачать компонент.";
  if (text.includes("Cannot write")) return "Не удалось записать файл.";
  if (text.includes("Cannot read")) return "Не удалось прочитать файл.";
  return text;
}

function translateDom(root: HTMLElement) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const textNodes: Text[] = [];
  while (walker.nextNode()) textNodes.push(walker.currentNode as Text);
  for (const node of textNodes) {
    const parent = node.parentElement;
    if (!parent || ["SCRIPT", "STYLE", "CODE", "PRE"].includes(parent.tagName)) continue;
    const source = node.nodeValue ?? "";
    const translated = translate(source.trim(), "ru");
    if (translated !== source.trim() && source.trim()) {
      node.nodeValue = source.replace(source.trim(), translated);
    }
  }
  for (const element of Array.from(root.querySelectorAll<HTMLElement>("[placeholder],[aria-label],option"))) {
    if (element instanceof HTMLInputElement && element.placeholder) {
      element.placeholder = translate(element.placeholder, "ru");
    }
    const aria = element.getAttribute("aria-label");
    if (aria) element.setAttribute("aria-label", translate(aria, "ru"));
    if (element instanceof HTMLOptionElement) element.text = translate(element.text, "ru");
  }
}

export function useT() {
  const language = useContext(I18nContext);
  return (text: string | number | undefined | null) => {
    if (typeof text !== "string") return text;
    return translate(text, language);
  };
}
