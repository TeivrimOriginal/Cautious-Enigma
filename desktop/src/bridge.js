/**
 * Мост между офлайн-сайтом и десктоп-оболочкой.
 *
 * Скрипт подставляется в WebView до загрузки страницы, поэтому приложение
 * может сразу пользоваться `window.linguarustDesktop`. В обычном браузере
 * объект отсутствует, и сайт продолжает работать как статическая версия.
 *
 * Каждый запрос моста подписан одноразовым токеном запуска: случайный порт
 * localhost доступен любому локальному процессу, а токен живёт только внутри
 * этого окна и защищает настройки и файлы прогресса.
 */
(() => {
  const TOKEN = "__LINGUARUST_TOKEN__";

  const call = async (action, body) => {
    const response = await fetch(`/__app/${action}`, {
      method: body === undefined ? "GET" : "POST",
      headers: { "Content-Type": "application/json", "X-LinguaRust-Token": TOKEN },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || `${action}: HTTP ${response.status}`);
    return data;
  };

  const bridge = {
    available: true,
    info: null,

    /** Сведения о сборке и каталоге данных; вызывается один раз при старте. */
    init: async () => (bridge.info = await call("info")),

    /** Отдать последнюю резервную копию прогресса на диск. */
    backup: (payload) => call("backup", { payload }),

    /** Сохранить отдельный файл экспорта с датой в имени. */
    export: (payload) => call("export", { payload }),

    /** Прочитать последнюю резервную копию с диска. */
    restore: () => call("backup"),

    /** Включить или выключить автозапуск вместе с Windows. */
    autostart: (enabled) => call("autostart", { enabled }),

    /** Синхронизировать тему заголовка окна с темой приложения. */
    theme: (theme) => call("theme", { theme }).catch(() => ({})),

    /** Открыть каталог данных в Проводнике. */
    openDataDir: () => call("open-data-dir", {}),
  };

  window.linguarustDesktop = bridge;
})();
