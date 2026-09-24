// LinguaRust — весь интерактив на ~150 строк без внешних зависимостей.
// Сервер отдаёт готовый HTML; JS добавляет только перевод по клику в текстах
// и проверку грамматики. Повторение карточек работает без JS (details + submit).

(function () {
  "use strict";

  const api = {
    async translate(word) {
      const res = await fetch(`/api/translate?word=${encodeURIComponent(word)}`);
      if (res.status === 404) return { missing: true };
      if (!res.ok) throw new Error("translate failed");
      return res.json();
    },
    async addCard(front, back, example) {
      const res = await fetch("/api/cards", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ front, back, example }),
      });
      if (res.status === 401) return { unauthorized: true };
      if (!res.ok) throw new Error("add card failed");
      return res.json();
    },
    async checkGrammar(id, answer) {
      const res = await fetch("/api/grammar/check", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id, answer }),
      });
      if (!res.ok) throw new Error("check failed");
      return res.json();
    },
  };

  /* ---------- Произношение (Web Speech API) ---------- */

  function speak(word) {
    if (!("speechSynthesis" in window)) return;
    window.speechSynthesis.cancel();
    const utterance = new SpeechSynthesisUtterance(word);
    utterance.lang = "en-GB";
    utterance.rate = 0.95;

    const voices = window.speechSynthesis.getVoices();
    const english =
      voices.find(
        (voice) =>
          voice.lang?.toLowerCase().startsWith("en") &&
          /female|samantha|karen|zira/i.test(voice.name),
      ) || voices.find((voice) => voice.lang?.toLowerCase().startsWith("en"));
    if (english) utterance.voice = english;

    window.speechSynthesis.speak(utterance);
  }

  function speakButton(word) {
    return `<button class="icon-btn" type="button" data-speak="${escapeHtml(word)}"
      title="Произнести" aria-label="Произнести">🔊</button>`;
  }

  /* ---------- Чтение: перевод слова по клику ---------- */

  function splitWords(root) {
    const pattern = /([A-Za-z]+(?:['’-][A-Za-z]+)*)/g;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
    const nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);

    for (const node of nodes) {
      const text = node.textContent;
      if (!pattern.test(text)) continue;
      pattern.lastIndex = 0;

      const fragment = document.createDocumentFragment();
      let last = 0;
      let match;
      while ((match = pattern.exec(text)) !== null) {
        if (match.index > last) {
          fragment.appendChild(document.createTextNode(text.slice(last, match.index)));
        }
        const span = document.createElement("span");
        span.className = "word";
        span.dataset.word = match[1];
        span.textContent = match[1];
        fragment.appendChild(span);
        last = match.index + match[0].length;
      }
      if (last < text.length) {
        fragment.appendChild(document.createTextNode(text.slice(last)));
      }
      node.parentNode.replaceChild(fragment, node);
    }
  }

  function initReader() {
    const reader = document.querySelector("[data-reader]");
    if (!reader) return;

    splitWords(reader);

    const popup = document.createElement("div");
    popup.className = "popup";
    popup.hidden = true;
    document.body.appendChild(popup);

    const hide = () => {
      popup.hidden = true;
    };

    const spans = [...reader.querySelectorAll(".word")];

    /** Сначала пробуем фразы из двух слов («alarm clock»), затем одно слово. */
    const candidatesFor = (span) => {
      const index = spans.indexOf(span);
      const candidates = [];
      if (index > 0) candidates.push({ text: `${spans[index - 1].dataset.word} ${span.dataset.word}`, spans: [spans[index - 1], span] });
      if (index >= 0) {
        candidates.push({
          text: `${span.dataset.word} ${spans[index + 1]?.dataset.word ?? ""}`.trim(),
          spans: [span, spans[index + 1]].filter(Boolean),
        });
      }
      candidates.push({ text: span.dataset.word, spans: [span] });
      return candidates;
    };

    document.addEventListener("click", async (event) => {
      const speaker = event.target.closest("[data-speak]");
      if (speaker) {
        event.stopPropagation();
        speak(speaker.dataset.speak);
        return;
      }

      const target = event.target.closest(".word");
      if (!target) {
        if (!event.target.closest(".popup")) hide();
        return;
      }

      const word = target.dataset.word;
      popup.hidden = false;
      popup.innerHTML = `<h4>${escapeHtml(word)}</h4><div class="muted small">Загрузка…</div>`;
      positionPopup(popup, target);

      for (const candidate of candidatesFor(target)) {
        let data;
        try {
          data = await api.translate(candidate.text);
        } catch (err) {
          break;
        }
        if (data.missing) continue;

        if (data.in_cards) {
          candidate.spans.forEach((span) => span.classList.add("known"));
        }
        popup.innerHTML = `
          <h4>${escapeHtml(data.word)} ${speakButton(data.word)}</h4>
          <div class="translation">${escapeHtml(data.translation)}</div>
          ${data.example ? `<div class="example">${escapeHtml(data.example)}</div>` : ""}
          ${data.can_add ? '<button class="btn small" data-add>В карточки</button>' : ""}
          ${data.in_cards ? '<div class="popup-status">Уже в ваших карточках</div>' : ""}
        `;
        const addButton = popup.querySelector("[data-add]");
        if (addButton) {
          addButton.addEventListener("click", async () => {
            addButton.disabled = true;
            addButton.textContent = "Добавляю…";
            const result = await api.addCard(data.word, data.translation, data.example);
            if (result.unauthorized) {
              addButton.textContent = "Нужен профиль";
              return;
            }
            candidate.spans.forEach((span) => span.classList.add("known"));
            addButton.textContent = result.added ? "Добавлено ✓" : "Уже было";
          });
        }
        return;
      }

      popup.innerHTML = `<h4>${escapeHtml(word)}</h4>
        <div class="translation muted">В словаре пока нет перевода</div>`;
    });

    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") hide();
    });
    window.addEventListener("scroll", hide, { passive: true });
  }

  function positionPopup(popup, anchor) {
    const rect = anchor.getBoundingClientRect();
    const width = popup.offsetWidth || 260;
    const left = Math.min(Math.max(8, rect.left + window.scrollX - width / 2), window.scrollX + document.documentElement.clientWidth - width - 8);
    popup.style.left = `${left}px`;
    popup.style.top = `${rect.bottom + window.scrollY + 8}px`;
  }

  function escapeHtml(value) {
    const div = document.createElement("div");
    div.textContent = value == null ? "" : String(value);
    return div.innerHTML;
  }

  /* ---------- Грамматика: проверка ответа ---------- */

  function initGrammar() {
    const form = document.querySelector("[data-grammar]");
    if (!form) return;

    const feedback = document.querySelector("[data-grammar-feedback]");
    const options = Array.from(form.querySelectorAll(".option"));

    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      const selected = form.querySelector('input[name="answer"]:checked');
      if (!selected) {
        feedback.innerHTML = '<div class="notice err">Выберите вариант ответа</div>';
        return;
      }

      const id = Number(form.dataset.exerciseId);
      const answer = Number(selected.value);
      feedback.innerHTML = '<div class="notice">Проверяю…</div>';

      try {
        const result = await api.checkGrammar(id, answer);
        options.forEach((option, index) => {
          option.classList.remove("correct", "wrong");
          if (index === result.correct_index) option.classList.add("correct");
          if (index === answer && !result.correct) option.classList.add("wrong");
        });

        feedback.innerHTML = `
          <div class="notice ${result.correct ? "ok" : "err"}">
            ${result.correct ? "Верно!" : `Неверно. Правильный ответ: ${escapeHtml(result.correct_option)}`}
          </div>
          <div class="explanation">${escapeHtml(result.explanation)}</div>
        `;
      } catch (err) {
        feedback.innerHTML = '<div class="notice err">Проверка недоступна, попробуйте позже</div>';
      }
    });
  }

  document.addEventListener("DOMContentLoaded", () => {
    initReader();
    initGrammar();
  });
})();
