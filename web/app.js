// LinguaRust — статическая версия сайта.
//
// Работает без сервера: словарь, тексты и упражнения загружаются как файлы,
// прогресс хранится в localStorage. Алгоритм повторений — тот же SM-2,
// что и в серверной версии (src/sm2.rs).

(function () {
  "use strict";

  const STORAGE_KEY = "linguarust.v1";
  const MIN_EASE = 1.3;
  const REVIEW_QUALITIES = [
    { value: 1, label: "Опять", css: "bad" },
    { value: 3, label: "Сложно", css: "warn" },
    { value: 4, label: "Нормально", css: "soft" },
    { value: 5, label: "Легко", css: "good" },
  ];

  const app = document.getElementById("app");
  const profileBox = document.getElementById("profile-box");

  /* ------------------------------------------------------------------ *
   * Утилиты
   * ------------------------------------------------------------------ */

  const pad = (n) => String(n).padStart(2, "0");

  function today() {
    const now = new Date();
    return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
  }

  function addDays(date, days) {
    const [y, m, d] = date.split("-").map(Number);
    const target = new Date(y, m - 1, d + days);
    return `${target.getFullYear()}-${pad(target.getMonth() + 1)}-${pad(target.getDate())}`;
  }

  function daysBetween(a, b) {
    const [ay, am, ad] = a.split("-").map(Number);
    const [by, bm, bd] = b.split("-").map(Number);
    return Math.round((new Date(by, bm - 1, bd) - new Date(ay, am - 1, ad)) / 86400000);
  }

  function escapeHtml(value) {
    return String(value == null ? "" : value).replace(/[&<>"']/g, (ch) => ({
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    })[ch]);
  }

  function formatInterval(days) {
    if (days <= 0) return "сейчас";
    if (days === 1) return "через 1 день";
    if (days <= 4) return `через ${days} дня`;
    if (days <= 20) return `через ${days} дней`;
    if (days <= 365) return `через ${Math.round(days / 30)} мес.`;
    return `через ${(days / 365).toFixed(1)} года`;
  }

  const mastery = (repetitions) =>
    repetitions <= 0 ? 0 : Math.min(repetitions, 5) * 20;

  /* ------------------------------------------------------------------ *
   * Произношение (Web Speech API, работает офлайн)
   * ------------------------------------------------------------------ */

  function speak(word) {
    if (!("speechSynthesis" in window)) return;
    window.speechSynthesis.cancel();
    const utterance = new SpeechSynthesisUtterance(word);
    utterance.lang = "en-GB";
    utterance.rate = 0.95;

    const voices = window.speechSynthesis.getVoices();
    const english = voices.find(
      (voice) => voice.lang?.toLowerCase().startsWith("en") && /female|samantha|karen|zira/i.test(voice.name),
    ) || voices.find((voice) => voice.lang?.toLowerCase().startsWith("en"));
    if (english) utterance.voice = english;

    window.speechSynthesis.speak(utterance);
  }

  /** Кнопка озвучивания для карточек и словаря. */
  const speakButton = (word) =>
    `<button class="icon-btn" data-speak="${escapeHtml(word)}" title="Произнести" aria-label="Произнести">🔊</button>`;

  function accuracy(total, successful) {
    if (total <= 0) return 0;
    return Math.min(100, (successful / total) * 100);
  }

  /* ------------------------------------------------------------------ *
   * Хранилище (localStorage)
   * ------------------------------------------------------------------ */

  const emptyState = () => ({
    profile: null,
    cards: [],
    reviews: [],
    grammar: [],
    knownWords: [],
  });

  let state = emptyState();

  function loadState() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      state = raw ? { ...emptyState(), ...JSON.parse(raw) } : emptyState();
    } catch (err) {
      state = emptyState();
    }
  }

  function saveState() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch (err) {
      console.warn("не удалось сохранить прогресс", err);
    }
  }

  /* ------------------------------------------------------------------ *
   * Данные: словарь, тексты, упражнения
   * ------------------------------------------------------------------ */

  const data = { dictionary: new Map(), entries: [], texts: [], exercises: [] };

  async function loadData() {
    const [tsv, texts, grammar] = await Promise.all([
      fetch("data/dictionary.tsv").then((r) => r.text()),
      fetch("data/texts.json").then((r) => r.json()),
      fetch("data/grammar.json").then((r) => r.json()),
    ]);
    data.dictionary = buildDictionary(tsv);
    data.entries = [...new Map([...data.dictionary.values()].map((entry) => [entry.front, entry])).values()]
      .sort((a, b) => a.front.toLowerCase().localeCompare(b.front.toLowerCase()));
    data.texts = texts;
    data.exercises = grammar;
  }

  const normalize = (word) =>
    word
      .trim()
      .replace(/^[^A-Za-zÀ-ɏ'’-]+|[^A-Za-zÀ-ɏ'’-]+$/g, "")
      .toLowerCase();

  function buildDictionary(tsv) {
    const map = new Map();
    for (const line of tsv.split(/\r?\n/)) {
      const row = line.trim();
      if (!row || row.startsWith("#")) continue;
      const [front, back, example] = row.split("\t");
      if (!front || !back) continue;
      const entry = { front, back: back.trim(), example: (example || "").trim() };
      map.set(normalize(front), entry);
      if (front.startsWith("to ")) {
        map.set(normalize(front.slice(3)), entry);
      }
    }
    return map;
  }

  /** Простые словоформы: books → book, running → run, delays → delay. */
  function candidateForms(word) {
    const forms = [];
    const push = (stem) => {
      if (stem && stem !== word) forms.push(stem);
    };

    if (word.endsWith("ies")) push(`${word.slice(0, -3)}y`);
    ["ing", "ed", "es", "s"].forEach((suffix) => {
      if (!word.endsWith(suffix)) return;
      const stem = word.slice(0, -suffix.length);
      push(stem);
      push(`${stem}e`);
      if (stem.length > 2 && stem.at(-1) === stem.at(-2) && !"aeiou".includes(stem.at(-1))) {
        push(stem.slice(0, -1));
      }
    });
    if (word.endsWith("ly")) push(word.slice(0, -2));

    return [...new Set(forms)];
  }

  function lookupWord(raw) {
    const key = normalize(raw);
    if (!key) return null;
    return data.dictionary.get(key) || candidateForms(key).map((f) => data.dictionary.get(f)).find(Boolean) || null;
  }

  /** Слово дня: стабильный выбор по номеру дня в году. */
  function wordOfTheDay() {
    if (!data.entries.length) return null;
    const now = new Date();
    const start = new Date(now.getFullYear(), 0, 1);
    const dayOfYear = Math.floor((now - start) / 86400000);
    return data.entries[dayOfYear % data.entries.length];
  }

  /* ------------------------------------------------------------------ *
   * SM-2
   * ------------------------------------------------------------------ */

  function reviewCard(card, quality) {
    const q = Math.max(0, Math.min(5, quality));
    let { repetitions, intervalDays, ease } = card;

    if (q >= 3) {
      repetitions += 1;
      if (repetitions === 1) intervalDays = 1;
      else if (repetitions === 2) intervalDays = 6;
      else intervalDays = Math.max(1, Math.round(intervalDays * ease));
    } else {
      repetitions = 0;
      intervalDays = 1;
    }

    ease = Math.max(MIN_EASE, ease + 0.1 - (5 - q) * (0.08 + (5 - q) * 0.02));
    return { ...card, repetitions, intervalDays, ease, dueDate: addDays(today(), intervalDays), lastReviewedAt: new Date().toISOString() };
  }

  /* ------------------------------------------------------------------ *
   * Карточки
   * ------------------------------------------------------------------ */

  function dueCards() {
    const day = today();
    return state.cards
      .filter((card) => card.dueDate && card.dueDate <= day)
      .sort((a, b) => a.dueDate.localeCompare(b.dueDate) || a.repetitions - b.repetitions);
  }

  function addCard(front, back, example) {
    const word = front.trim();
    if (!word || !back.trim()) return false;
    if (state.cards.some((card) => card.front.toLowerCase() === word.toLowerCase())) return false;

    state.cards.push({
      id: Date.now() + Math.floor(Math.random() * 1000),
      front: word,
      back: back.trim(),
      example: (example || "").trim(),
      repetitions: 0,
      intervalDays: 0,
      ease: 2.5,
      dueDate: today(),
      createdAt: new Date().toISOString(),
    });
    saveState();
    return true;
  }

  function gradeCard(cardId, quality) {
    const index = state.cards.findIndex((card) => card.id === cardId);
    if (index < 0) return;
    state.cards[index] = reviewCard(state.cards[index], quality);
    state.reviews.push({ cardId, quality, day: today() });
    saveState();
  }

  function removeCard(cardId) {
    state.cards = state.cards.filter((card) => card.id !== cardId);
    saveState();
  }

  /* ------------------------------------------------------------------ *
   * Статистика
   * ------------------------------------------------------------------ */

  function stats() {
    const days = [...new Set(state.reviews.map((review) => review.day))].sort();
    const day = today();
    const yesterday = addDays(day, -1);

    let current = 0;
    let cursor = days.includes(day) ? day : days.includes(yesterday) ? yesterday : null;
    if (cursor) {
      while (days.includes(cursor)) {
        current += 1;
        cursor = addDays(cursor, -1);
      }
    }

    let longest = 0;
    let run = 0;
    days.forEach((currentDay, index) => {
      run = index > 0 && daysBetween(days[index - 1], currentDay) === 1 ? run + 1 : 1;
      longest = Math.max(longest, run);
    });

    const totalReviews = state.reviews.length;
    const successful = state.reviews.filter((review) => review.quality >= 3).length;
    const learned = state.cards.filter((card) => card.repetitions > 0).length;
    const byDay = new Map();
    state.reviews.forEach((review) => byDay.set(review.day, (byDay.get(review.day) || 0) + 1));

    return {
      current,
      longest,
      totalReviews,
      successful,
      accuracy: accuracy(totalReviews, successful),
      totalCards: state.cards.length,
      learned,
      fresh: state.cards.length - learned,
      due: dueCards().length,
      todayReviews: byDay.get(day) || 0,
      daysActive: days.length,
      perDay: days.length ? totalReviews / days.length : 0,
      byDay,
    };
  }

  /* ------------------------------------------------------------------ *
   * Представления
   * ------------------------------------------------------------------ */

  function renderWelcome() {
    return `
      <section class="panel" style="max-width:560px;margin:40px auto;text-align:center">
        <h1 style="margin-bottom:6px">Привет! Это LinguaRust</h1>
        <p class="muted">
          Учите английский по карточкам с интервальным повторением, читайте тексты
          с переводом по клику и тренируйте грамматику.
        </p>
        <form id="welcome-form">
          <label class="field" style="text-align:left">
            <span>Как вас зовут?</span>
            <input type="text" name="name" maxlength="40" placeholder="Например, Алекс" required autofocus />
          </label>
          <button class="btn" type="submit">Начать обучение</button>
        </form>
        <p class="muted small" style="margin-top:22px;margin-bottom:0">
          Регистрации и пароля нет: прогресс хранится только в вашем браузере.<br />
          Очистка данных сайта удалит карточки и статистику.
        </p>
      </section>`;
  }

  function renderHome() {
    const s = stats();
    const days = [];
    for (let i = 13; i >= 0; i -= 1) days.push(addDays(today(), -i));
    const max = Math.max(1, ...days.map((day) => s.byDay.get(day) || 0));

    return `
      <section class="panel">
        <h1 style="margin-bottom:4px">Привет, ${escapeHtml(state.profile.name)}!</h1>
        <p class="muted" style="margin-top:0">
          ${s.due > 0
            ? `Сегодня к повторению — ${s.due} карточек. Повторения важнее новых слов: так лексика остаётся в памяти.`
            : "На сегодня всё повторено. Можно добавить новые слова или почитать текст."}
        </p>
        <div class="actions">
          <a class="btn" href="#/cards">${s.due > 0 ? `Повторить ${s.due}` : "Открыть карточки"}</a>
          <a class="btn ghost" href="#/reading">Читать</a>
          <a class="btn ghost" href="#/grammar">Грамматика</a>
        </div>
      </section>

      <section class="grid cols-4">
        <div class="metric"><div class="value accent">${s.current}</div><div class="label">дней подряд</div></div>
        <div class="metric"><div class="value">${s.accuracy.toFixed(1)}%</div><div class="label">точность ответов</div></div>
        <div class="metric"><div class="value">${s.learned}</div><div class="label">слов в работе</div></div>
        <div class="metric"><div class="value">${s.todayReviews}</div><div class="label">повторений сегодня</div></div>
      </section>

      <section class="panel" style="margin-top:20px">
        <h3>Активность за 14 дней</h3>
        <div class="chart">
          ${days
            .map((day) => {
              const count = s.byDay.get(day) || 0;
              const percent = Math.round((count / max) * 100);
              return `<div class="col" title="${day}: ${count}">
                  <div class="bar" style="height:${percent}%"></div>
                  <div class="label">${day.slice(8, 10)}.${day.slice(5, 7)}</div>
                </div>`;
            })
            .join("")}
        </div>
        <p class="muted small">Высота столбика — число повторений за день.</p>
      </section>`;
  }

  function renderCards() {
    const due = dueCards();
    const s = stats();
    const card = due[0];

    const review = card
      ? `<section class="panel review-card">
          <p class="muted small" style="margin:0">Карточек к повторению: ${due.length}</p>
          <div class="word">${escapeHtml(card.front)}</div>
          <details class="reveal">
            <summary class="btn ghost">Показать перевод</summary>
            <div class="answer">${escapeHtml(card.back)}</div>
            ${card.example ? `<div class="example">${escapeHtml(card.example)}</div>` : ""}
          </details>
          <div class="quality-grid">
            ${REVIEW_QUALITIES.map(
              (item) =>
                `<button class="btn ${item.css}" data-grade="${item.value}" data-card="${card.id}">${item.label}</button>`,
            ).join("")}
          </div>
          <p class="muted small" style="margin-top:14px;margin-bottom:0">
            Пробел — показать ответ, клавиши 1–4 — оценка по порядку кнопок.
          </p>
        </section>`
      : `<section class="panel" style="text-align:center">
          <h2>На сегодня повторений нет 🎉</h2>
          <p class="muted">Добавьте новые слова — они попадут в очередь на завтра.</p>
          <a class="btn" href="#add">Добавить слово</a>
        </section>`;

    const rows = [...state.cards]
      .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
      .map((card) => {
        const percent = mastery(card.repetitions);
        const isDue = card.dueDate && card.dueDate <= today();
        return `<li>
            <div class="card-front">${escapeHtml(card.front)}</div>
            <div class="card-back">${escapeHtml(card.back)}</div>
            <div class="mastery" title="освоено на ${percent}%"><span style="width:${percent}%"></span></div>
            <span class="tag ${isDue ? "due" : ""}">${isDue ? "к повторению" : escapeHtml(card.dueDate)}</span>
            <span class="tag">${formatInterval(card.intervalDays)}</span>
            <button class="btn ghost small" data-delete="${card.id}">Удалить</button>
          </li>`;
      })
      .join("");

    return `
      ${review}
      <section class="panel" id="add">
        <h3>Новое слово</h3>
        <form id="add-form">
          <div class="grid cols-3">
            <label class="field"><span>Слово (EN)</span>
              <input type="text" name="front" maxlength="100" placeholder="deadline" required /></label>
            <label class="field"><span>Перевод (RU)</span>
              <input type="text" name="back" maxlength="200" placeholder="срок сдачи" required /></label>
            <label class="field"><span>Пример</span>
              <input type="text" name="example" maxlength="300" placeholder="The deadline is Friday." /></label>
          </div>
          <button class="btn" type="submit">Добавить карточку</button>
        </form>
      </section>

      <section class="panel">
        <h3>Все карточки (${s.totalCards})</h3>
        ${rows ? `<ul class="card-list">${rows}</ul>` : '<p class="muted">Пока пусто. Начните с текста для чтения — слова оттуда добавляются по клику.</p>'}
      </section>`;
  }

  function renderDictionary() {
    const wotd = wordOfTheDay();
    const inCards = (front) => state.cards.some((card) => normalize(card.front) === normalize(front));

    const card = wotd
      ? `<section class="panel" style="display:flex;gap:18px;align-items:center;flex-wrap:wrap">
          <div style="flex:1;min-width:220px">
            <span class="tag">Слово дня</span>
            <h2 style="margin:8px 0 4px">${escapeHtml(wotd.front)}</h2>
            <p style="margin:0;font-size:18px">${escapeHtml(wotd.back)}</p>
            ${wotd.example ? `<p class="muted small" style="margin:6px 0 0">${escapeHtml(wotd.example)}</p>` : ""}
          </div>
          ${
            inCards(wotd.front)
              ? '<span class="tag ok">уже в карточках</span>'
              : `<button class="btn" data-add-front="${escapeHtml(wotd.front)}">В карточки</button>`
          }
        </section>`
      : "";

    const rows = data.entries
      .map((entry) => {
        const known = inCards(entry.front);
        return `<li data-front="${escapeHtml(entry.front.toLowerCase())}" data-back="${escapeHtml(entry.back.toLowerCase())}">
            <div class="card-front">${escapeHtml(entry.front)} ${speakButton(entry.front)}</div>
            <div class="card-back">
              ${escapeHtml(entry.back)}
              ${entry.example ? `<div class="muted small">${escapeHtml(entry.example)}</div>` : ""}
            </div>
            ${
              known
                ? '<span class="tag ok">в карточках</span>'
                : `<button class="btn ghost small" data-add-front="${escapeHtml(entry.front)}">В карточки</button>`
            }
          </li>`;
      })
      .join("");

    return `
      ${card}
      <section class="panel">
        <div class="actions" style="justify-content:space-between">
          <h1 style="margin:0">Словарь</h1>
          <span class="tag">слов: ${data.entries.length}</span>
        </div>
        <div class="actions">
          <input type="text" id="dict-search" placeholder="Поиск по английскому или русскому…"
                 style="flex:1;min-width:220px" autocomplete="off" />
          <span class="muted small" id="dict-count">${data.entries.length}</span>
        </div>
        <ul class="card-list" id="dict-list">${rows}</ul>
      </section>`;
  }

  function initDictionary() {
    const search = document.getElementById("dict-search");
    if (!search) return;

    const apply = () => {
      const needle = search.value.trim().toLowerCase();
      let visible = 0;
      document.querySelectorAll("#dict-list li").forEach((item) => {
        const match =
          !needle ||
          item.dataset.front.includes(needle) ||
          item.dataset.back.includes(needle);
        item.hidden = !match;
        if (match) visible += 1;
      });
      document.getElementById("dict-count").textContent = String(visible);
    };

    search.addEventListener("input", apply);
    search.focus();
  }

  function renderReading() {
    const cards = data.texts
      .map(
        (text) => `
        <section class="panel" style="margin-bottom:0">
          <div class="actions" style="justify-content:space-between">
            <h3 style="margin:0">${escapeHtml(text.title)}</h3>
            <span class="tag">${escapeHtml(text.level)}</span>
          </div>
          <p class="muted small">${escapeHtml(text.summary)}</p>
          <p class="muted small">${text.content.trim().split(/\s+/).length} слов</p>
          <a class="btn small" href="#/reading/${encodeURIComponent(text.slug)}">Читать</a>
        </section>`,
      )
      .join("");

    return `
      <section class="panel">
        <h1>Тексты для чтения</h1>
        <p class="muted" style="margin-top:0">Нажмите на любое слово — появится перевод и кнопка «В карточки».</p>
      </section>
      <div class="grid cols-2">${cards}</div>`;
  }

  function renderReadingText(slug) {
    const text = data.texts.find((item) => item.slug === slug);
    if (!text) return '<section class="panel"><h1>Текст не найден</h1><p><a href="#/reading">К списку текстов</a></p></section>';

    const paragraphs = text.content
      .split(/\n\s*\n/)
      .map((paragraph) => `<p>${escapeHtml(paragraph.trim())}</p>`)
      .join("");

    const others = data.texts
      .filter((item) => item.slug !== slug)
      .map((item) => `<a class="btn ghost small" href="#/reading/${encodeURIComponent(item.slug)}">${escapeHtml(item.title)} · ${escapeHtml(item.level)}</a>`)
      .join("");

    return `
      <article class="panel">
        <div class="actions" style="justify-content:space-between">
          <h1 style="margin:0">${escapeHtml(text.title)}</h1>
          <div><span class="tag">${escapeHtml(text.level)}</span>
            <span class="tag">${text.content.trim().split(/\s+/).length} слов</span></div>
        </div>
        <p class="muted small">${escapeHtml(text.summary)}</p>
        <div class="reader" id="reader">${paragraphs}</div>
        <p class="muted small">Слова, которые уже есть в карточках, подчёркиваются пунктиром.</p>
      </article>
      ${others ? `<section class="panel"><h3>Другие тексты</h3><div class="actions">${others}</div></section>` : ""}`;
  }

  function renderGrammar() {
    const index = Math.max(0, Math.min(data.exercises.length - 1, Number(location.hash.split("=")[1] || 0)));
    const exercise = data.exercises[index];
    if (!exercise) return '<section class="panel"><p class="muted">Упражнения не загружены.</p></section>';

    return `
      <section class="panel">
        <div class="actions" style="justify-content:space-between">
          <h1 style="margin:0">Грамматика</h1>
          <span class="tag">Упражнение ${index + 1} из ${data.exercises.length}</span>
        </div>
        <p class="muted small" style="margin-bottom:18px">Тема: ${escapeHtml(exercise.topic)}</p>
        <p style="font-size:20px;font-weight:600">${escapeHtml(exercise.prompt)}</p>
        <div id="grammar-options">
          ${exercise.options
            .map(
              (option, i) => `<label class="option">
                <input type="radio" name="answer" value="${i}" /> ${escapeHtml(option)}
              </label>`,
            )
            .join("")}
        </div>
        <div class="actions">
          <button class="btn" id="grammar-check">Проверить</button>
          <a class="btn ghost" href="#/grammar=${index + 1}">Следующее упражнение →</a>
        </div>
        <div id="grammar-feedback"></div>
      </section>`;
  }

  function renderStats() {
    const s = stats();
    const buckets = [
      { label: "Новые", count: s.fresh },
      { label: "Изучаются", count: state.cards.filter((c) => c.repetitions > 0 && c.repetitions <= 2).length },
      { label: "Знакомые", count: state.cards.filter((c) => c.repetitions > 3 && c.repetitions <= 4).length },
      { label: "Освоенные", count: state.cards.filter((c) => c.repetitions >= 5).length },
    ];
    const total = Math.max(1, s.totalCards);

    const rows = [...s.byDay.entries()]
      .sort((a, b) => b[0].localeCompare(a[0]))
      .slice(0, 14)
      .map(([day, reviews]) => {
        const successful = state.reviews.filter((review) => review.day === day && review.quality >= 3).length;
        return `<tr><td>${day}</td><td>${reviews}</td><td>${successful}</td>
          <td style="text-align:right">${Math.round(accuracy(reviews, successful))}%</td></tr>`;
      })
      .join("");

    return `
      <section class="panel">
        <h1>Статистика</h1>
        <p class="muted" style="margin-top:0">Стрики, точность и состояние карточек.</p>
      </section>

      <section class="grid cols-4">
        <div class="metric"><div class="value accent">${s.current}</div><div class="label">текущий стрик</div></div>
        <div class="metric"><div class="value">${s.longest}</div><div class="label">рекорд стрика</div></div>
        <div class="metric"><div class="value">${s.accuracy.toFixed(1)}%</div><div class="label">точность (${s.successful} из ${s.totalReviews})</div></div>
        <div class="metric"><div class="value">${s.perDay.toFixed(1)}</div><div class="label">повторений в день</div></div>
      </section>

      <section class="grid cols-2" style="margin-top:20px">
        <div class="panel">
          <h3>Карточки</h3>
          <table><tbody>
            <tr><td>Всего</td><td style="text-align:right">${s.totalCards}</td></tr>
            <tr><td>Новые</td><td style="text-align:right">${s.fresh}</td></tr>
            <tr><td>В работе</td><td style="text-align:right">${s.learned}</td></tr>
            <tr><td>К повторению</td><td style="text-align:right">${s.due}</td></tr>
            <tr><td>Активных дней</td><td style="text-align:right">${s.daysActive}</td></tr>
          </tbody></table>
        </div>
        <div class="panel">
          <h3>Стадии освоения</h3>
          ${buckets
            .map((bucket) => {
              const percent = Math.round((bucket.count / total) * 100);
              return `<div style="margin-bottom:12px">
                <div class="actions" style="justify-content:space-between">
                  <span>${bucket.label}</span>
                  <span class="muted small">${bucket.count} · ${percent}%</span>
                </div>
                <div class="progress"><span style="width:${percent}%"></span></div>
              </div>`;
            })
            .join("")}
        </div>
      </section>

      <section class="panel">
        <h3>Последние дни активности</h3>
        ${rows
          ? `<table><thead><tr><th>Дата</th><th>Повторений</th><th>Успешных</th><th style="text-align:right">Точность</th></tr></thead><tbody>${rows}</tbody></table>`
          : '<p class="muted">Пока нет повторений — начните с карточек.</p>'}
      </section>`;
  }

  /* ------------------------------------------------------------------ *
   * Роутер
   * ------------------------------------------------------------------ */

  const routes = [
    { pattern: /^#?\/?$/, view: "home" },
    { pattern: /^#\/cards$/, view: "cards" },
    { pattern: /^#\/dictionary$/, view: "dictionary" },
    { pattern: /^#\/reading$/, view: "reading" },
    { pattern: /^#\/reading\/(.+)$/, view: "readingText" },
    { pattern: /^#\/grammar(?:=(\d+))?$/, view: "grammar" },
    { pattern: /^#\/stats$/, view: "stats" },
  ];

  function currentRoute() {
    const hash = location.hash || "#/";
    for (const route of routes) {
      const match = hash.match(route.pattern);
      if (match) return { view: route.view, param: match[1] ? decodeURIComponent(match[1]) : null };
    }
    return { view: "home", param: null };
  }

  function renderProfileBox() {
    if (!state.profile) {
      profileBox.innerHTML = '<a class="btn small" href="#/">Начать</a>';
      return;
    }
    const s = stats();
    profileBox.innerHTML = `
      <span class="badge">🔥 ${s.current} дн.</span>
      ${s.due > 0 ? `<span class="badge hot">${s.due} к повторению</span>` : ""}
      <button class="btn ghost small" id="reset">Сбросить</button>`;
    document.getElementById("reset").addEventListener("click", () => {
      if (confirm("Удалить все карточки и статистику этого браузера?")) {
        localStorage.removeItem(STORAGE_KEY);
        state = emptyState();
        route();
      }
    });
  }

  function markActiveNav(view) {
    document.querySelectorAll("#nav a").forEach((link) => {
      const route = link.dataset.route;
      const active = route === view || (view === "readingText" && route === "reading");
      link.classList.toggle("active", active);
    });
  }

  function route() {
    const { view, param } = currentRoute();

    if (!state.profile) {
      markActiveNav("home");
      app.innerHTML = renderWelcome();
      renderProfileBox();
      return;
    }

    markActiveNav(view);
    switch (view) {
      case "cards":
        app.innerHTML = renderCards();
        break;
      case "reading":
        app.innerHTML = renderReading();
        break;
      case "dictionary":
        app.innerHTML = renderDictionary();
        initDictionary();
        break;
      case "readingText":
        app.innerHTML = renderReadingText(param);
        initReader();
        break;
      case "grammar":
        app.innerHTML = renderGrammar();
        break;
      case "stats":
        app.innerHTML = renderStats();
        break;
      default:
        app.innerHTML = renderHome();
    }
    renderProfileBox();
  }

  /* ------------------------------------------------------------------ *
   * Обработчики событий
   * ------------------------------------------------------------------ */

  document.addEventListener("submit", (event) => {
    const form = event.target;

    if (form.id === "welcome-form") {
      event.preventDefault();
      const name = new FormData(form).get("name").trim();
      if (!name) return;
      state.profile = { name, createdAt: new Date().toISOString() };
      saveState();
      location.hash = "#/cards";
      route();
      return;
    }

    if (form.id === "add-form") {
      event.preventDefault();
      const values = new FormData(form);
      const added = addCard(values.get("front"), values.get("back"), values.get("example"));
      if (!added) {
        app.insertAdjacentHTML("afterbegin", '<div class="notice err">Такое слово уже есть в карточках</div>');
      }
      route();
    }
  });

  document.addEventListener("click", (event) => {
    const grade = event.target.closest("[data-grade]");
    if (grade) {
      gradeCard(Number(grade.dataset.card), Number(grade.dataset.grade));
      route();
      return;
    }

    const remove = event.target.closest("[data-delete]");
    if (remove) {
      const card = state.cards.find((item) => item.id === Number(remove.dataset.delete));
      if (card && confirm(`Удалить карточку «${card.front}»?`)) {
        removeCard(card.id);
        route();
      }
      return;
    }

    const addFromDictionary = event.target.closest("[data-add-front]");
    if (addFromDictionary) {
      const entry = lookupWord(addFromDictionary.dataset.addFront);
      if (entry) {
        const added = addCard(entry.front, entry.back, entry.example);
        addFromDictionary.textContent = added ? "Добавлено ✓" : "Уже было";
        addFromDictionary.disabled = true;
      }
      return;
    }

    const speakTarget = event.target.closest("[data-speak]");
    if (speakTarget) {
      event.stopPropagation();
      speak(speakTarget.dataset.speak);
      return;
    }

    if (event.target.id === "grammar-check") {
      checkGrammar();
    }
  });

  function checkGrammar() {
    const hash = location.hash || "#/grammar";
    const index = Number(hash.split("=")[1] || 0);
    const exercise = data.exercises[index];
    const selected = document.querySelector('input[name="answer"]:checked');

    const feedback = document.getElementById("grammar-feedback");
    if (!selected) {
      feedback.innerHTML = '<div class="notice err">Выберите вариант ответа</div>';
      return;
    }

    const answer = Number(selected.value);
    const correct = answer === exercise.correct_index;

    document.querySelectorAll("#grammar-options .option").forEach((option, i) => {
      option.classList.toggle("correct", i === exercise.correct_index);
      option.classList.toggle("wrong", i === answer && !correct);
    });

    state.grammar.push({ exerciseId: index, correct, day: today() });
    saveState();

    feedback.innerHTML = `
      <div class="notice ${correct ? "ok" : "err"}">
        ${correct ? "Верно!" : `Неверно. Правильный ответ: ${escapeHtml(exercise.options[exercise.correct_index])}`}
      </div>
      <div class="explanation">${escapeHtml(exercise.explanation)}</div>`;
  }

  /* ------------------------------------------------------------------ *
   * Чтение: перевод слова по клику
   * ------------------------------------------------------------------ */

  function initReader() {
    const reader = document.getElementById("reader");
    if (!reader) return;

    const pattern = /([A-Za-zÀ-ɏ]+(?:['’-][A-Za-zÀ-ɏ]+)*)/g;
    reader.innerHTML = reader.innerHTML.replace(/>([^<]+)</g, (_, text) => {
      let html = "";
      let last = 0;
      let match;
      pattern.lastIndex = 0;
      while ((match = pattern.exec(text)) !== null) {
        html += escapeHtml(text.slice(last, match.index));
        const known = state.cards.some((card) => normalize(card.front) === normalize(match[1]));
        html += `<span class="word${known ? " known" : ""}" data-word="${escapeHtml(match[1])}">${escapeHtml(match[1])}</span>`;
        last = match.index + match[1].length;
      }
      html += escapeHtml(text.slice(last));
      return `>${html}<`;
    });

    const popup = document.createElement("div");
    popup.className = "popup";
    popup.hidden = true;
    document.body.appendChild(popup);

    const spans = [...reader.querySelectorAll(".word")];

    /** Какие слова подсветить при совпадении: одно или пара. */
    const matchedSpans = (candidate, span) => {
      if (!candidate.includes(" ")) return [span];
      const index = spans.indexOf(span);
      return candidate.split(" ")[0] === span.dataset.word
        ? [span, spans[index + 1]].filter(Boolean)
        : [spans[index - 1], span].filter(Boolean);
    };

    /** Сначала ищем фразы из двух слов («alarm clock»), затем одиночное слово. */
    const lookupInContext = (span) => {
      const index = spans.indexOf(span);
      const candidates = [];
      if (index > 0) candidates.push(`${spans[index - 1].dataset.word} ${span.dataset.word}`);
      if (index >= 0) candidates.push(`${span.dataset.word} ${spans[index + 1]?.dataset.word ?? ""}`.trim());
      candidates.push(span.dataset.word);

      for (const candidate of candidates) {
        const entry = lookupWord(candidate);
        if (entry) return { entry, spans: matchedSpans(candidate, span) };
      }
      return null;
    };

    document.addEventListener("click", (event) => {
      const word = event.target.closest(".word");
      if (!word) {
        if (!event.target.closest(".popup")) popup.hidden = true;
        return;
      }

      const found = lookupInContext(word);
      if (!found) {
        showPopup(popup, word, `<h4>${escapeHtml(word.dataset.word)}</h4>
          <div class="muted">В словаре пока нет перевода</div>`);
        return;
      }

      const entry = found.entry;
      const inCards = state.cards.some((card) => normalize(card.front) === normalize(entry.front));
      if (inCards) found.spans.forEach((span) => span.classList.add("known"));
      const button = inCards
        ? '<div class="popup-status">Уже в ваших карточках</div>'
        : '<button class="btn small" data-add-word>В карточки</button>';

      showPopup(popup, word, `<h4>${escapeHtml(entry.front)} ${speakButton(entry.front)}</h4>
        <div class="translation">${escapeHtml(entry.back)}</div>
        ${entry.example ? `<div class="example">${escapeHtml(entry.example)}</div>` : ""}
        ${button}`);

      const addButton = popup.querySelector("[data-add-word]");
      if (addButton) {
        addButton.addEventListener("click", () => {
          const added = addCard(entry.front, entry.back, entry.example);
          addButton.textContent = added ? "Добавлено ✓" : "Уже было";
          if (added) found.spans.forEach((span) => span.classList.add("known"));
        });
      }
    });

    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") popup.hidden = true;
    });
  }

  function showPopup(popup, anchor, html) {
    popup.innerHTML = html;
    popup.hidden = false;
    const rect = anchor.getBoundingClientRect();
    const width = popup.offsetWidth || 260;
    const left = Math.min(
      Math.max(8, rect.left + window.scrollX - width / 2),
      window.scrollX + document.documentElement.clientWidth - width - 8,
    );
    popup.style.left = `${left}px`;
    popup.style.top = `${rect.bottom + window.scrollY + 8}px`;
  }

  /* ------------------------------------------------------------------ *
   * Горячие клавиши
   * ------------------------------------------------------------------ */

  const QUALITY_BY_KEY = { 1: 1, 2: 3, 3: 4, 4: 5 };

  function initShortcuts() {
    document.addEventListener("keydown", (event) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return;

      const focused = event.target;
      const isTyping =
        focused instanceof HTMLElement &&
        (focused.tagName === "INPUT" || focused.tagName === "TEXTAREA");

      if (isTyping) {
        if (event.key === "Escape") focused.blur();
        return;
      }

      const { view } = currentRoute();

      if (event.key === "/") {
        const search = document.getElementById("dict-search");
        if (search) {
          event.preventDefault();
          search.focus();
        }
        return;
      }

      if (view === "cards") {
        if (event.key === " " || event.key === "Enter") {
          const reveal = document.querySelector("details.reveal");
          if (reveal) {
            event.preventDefault();
            reveal.open = !reveal.open;
          }
          return;
        }
        const quality = QUALITY_BY_KEY[event.key];
        if (quality) {
          const card = dueCards()[0];
          if (card) {
            event.preventDefault();
            gradeCard(card.id, quality);
            route();
          }
        }
        return;
      }

      if (view === "grammar") {
        const index = Number(event.key) - 1;
        const option = document.querySelectorAll('input[name="answer"]')[index];
        if (option) {
          event.preventDefault();
          option.checked = true;
        }
      }
    });
  }

  /* ------------------------------------------------------------------ *
   * Запуск
   * ------------------------------------------------------------------ */

  async function start() {
    loadState();
    try {
      await loadData();
    } catch (err) {
      app.innerHTML = `<section class="panel"><h1>Не удалось загрузить данные</h1>
        <p class="muted">Проверьте, что файлы <code>data/</code> доступны рядом с index.html.</p>
        <p class="muted small">${escapeHtml(err.message)}</p></section>`;
      return;
    }

    window.addEventListener("hashchange", route);
    initShortcuts();
    route();

    // Офлайн-режим: работает только на https и localhost.
    if ("serviceWorker" in navigator) {
      navigator.serviceWorker.register("sw.js").catch((err) => {
        console.warn("service worker не зарегистрирован", err);
      });
    }
  }

  start();
})();
