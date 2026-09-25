package com.linguarust.mobile;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Context;
import android.content.DialogInterface;
import android.content.res.Configuration;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.os.Bundle;
import android.text.Editable;
import android.text.TextWatcher;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.List;
import java.util.Locale;

/** Главный экран нативной мобильной версии LinguaRust. */
public final class MainActivity extends Activity {
    private LinearLayout root;
    private LinearLayout content;
    private LinearLayout bottomNav;
    private TextView topTitle;
    private MobileStore store;
    private ContentRepository repository;

    private String screen = "home";
    private String selectedTab = "home";
    private CardState reviewingCard;
    private CardState lastReviewedCard;
    private boolean answerVisible;
    private EditText dictionarySearch;
    private LinearLayout dictionaryResults;
    private int grammarIndex;
    private boolean grammarAnswered;
    private int grammarChoice = -1;
    private ReadingText selectedText;

    private int background;
    private int surface;
    private int surfaceMuted;
    private int textColor;
    private int mutedColor;
    private int accent;
    private int border;
    private int good;
    private int bad;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        configureColors();
        Window window = getWindow();
        window.setStatusBarColor(background);
        window.setNavigationBarColor(background);

        repository = new ContentRepository(getAssets());
        store = new MobileStore(this, repository);
        buildShell();
        render();
    }

    private void configureColors() {
        boolean night = (getResources().getConfiguration().uiMode
                & Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES;
        background = night ? Color.rgb(16, 19, 27) : Color.rgb(244, 246, 251);
        surface = night ? Color.rgb(23, 27, 38) : Color.WHITE;
        surfaceMuted = night ? Color.rgb(30, 35, 48) : Color.rgb(235, 238, 247);
        textColor = night ? Color.rgb(232, 235, 244) : Color.rgb(28, 34, 51);
        mutedColor = night ? Color.rgb(157, 166, 188) : Color.rgb(94, 104, 128);
        accent = night ? Color.rgb(142, 162, 255) : Color.rgb(91, 111, 232);
        border = night ? Color.rgb(53, 61, 80) : Color.rgb(220, 225, 238);
        good = night ? Color.rgb(93, 211, 153) : Color.rgb(31, 145, 86);
        bad = night ? Color.rgb(255, 125, 137) : Color.rgb(204, 67, 78);
    }

    private void buildShell() {
        root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(background);
        setContentView(root);

        LinearLayout top = new LinearLayout(this);
        top.setGravity(Gravity.CENTER_VERTICAL);
        top.setPadding(dp(20), dp(12), dp(14), dp(12));
        top.setBackgroundColor(surface);
        topTitle = label("LinguaRust", 22, textColor);
        topTitle.setTypeface(Typeface.DEFAULT_BOLD);
        top.addView(topTitle, new LinearLayout.LayoutParams(0, dp(42), 1));

        Button profile = button("Профиль", v -> showProfile());
        profile.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
        top.addView(profile, new LinearLayout.LayoutParams(dp(92), dp(38)));
        root.addView(top, new LinearLayout.LayoutParams(-1, dp(66)));

        ScrollView scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setPadding(dp(16), dp(18), dp(16), dp(28));
        scroll.addView(content, new ViewGroup.LayoutParams(-1, -2));
        root.addView(scroll, new LinearLayout.LayoutParams(-1, 0, 1));

        bottomNav = new LinearLayout(this);
        bottomNav.setGravity(Gravity.CENTER);
        bottomNav.setPadding(dp(8), dp(6), dp(8), dp(8));
        bottomNav.setBackgroundColor(surface);
        addNavButton(bottomNav, "Главная", "home");
        addNavButton(bottomNav, "Карточки", "cards");
        addNavButton(bottomNav, "Словарь", "dictionary");
        addNavButton(bottomNav, "Ещё", "more");
        root.addView(bottomNav, new LinearLayout.LayoutParams(-1, dp(68)));
    }

    private void addNavButton(LinearLayout parent, String title, String key) {
        Button button = button(title, v -> navigate(key));
        button.setAllCaps(false);
        button.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
        button.setPadding(2, 2, 2, 2);
        parent.addView(button, new LinearLayout.LayoutParams(0, dp(52), 1));
    }

    private void navigate(String key) {
        selectedTab = key;
        if ("grammar".equals(key) || "reading".equals(key) || "stats".equals(key)
                || "settings".equals(key)) {
            screen = key;
        } else {
            screen = key;
        }
        if ("cards".equals(key)) {
            reviewingCard = null;
            answerVisible = false;
        }
        if ("grammar".equals(key)) {
            grammarIndex = 0;
            grammarAnswered = false;
            grammarChoice = -1;
        }
        render();
    }

    private void render() {
        content.removeAllViews();
        switch (screen) {
            case "cards":
                renderCards();
                break;
            case "dictionary":
                renderDictionary();
                break;
            case "more":
                renderMore();
                break;
            case "grammar":
                renderGrammar();
                break;
            case "reading":
                renderReadingList();
                break;
            case "reading-detail":
                renderReadingDetail();
                break;
            case "stats":
                renderStats();
                break;
            case "settings":
                renderSettings();
                break;
            default:
                renderHome();
                break;
        }
        highlightNavigation();
    }

    private void highlightNavigation() {
        if (bottomNav == null) return;
        for (int i = 0; i < bottomNav.getChildCount(); i++) {
            View child = bottomNav.getChildAt(i);
            if (!(child instanceof Button)) continue;
            Button nav = (Button) child;
            String title = nav.getText().toString();
            boolean active = ("home".equals(selectedTab) && "Главная".equals(title))
                    || ("cards".equals(selectedTab) && "Карточки".equals(title))
                    || ("dictionary".equals(selectedTab) && "Словарь".equals(title))
                    || ("more".equals(selectedTab) && "Ещё".equals(title));
            nav.setTextColor(active ? accent : mutedColor);
            nav.setBackground(round(active ? surfaceMuted : Color.TRANSPARENT, 12));
        }
    }

    private void renderHome() {
        topTitle.setText("LinguaRust");
        LinearLayout page = page();
        MobileStore.LevelInfo level = store.level();
        int due = store.dueCount();

        TextView hello = label("Привет, " + store.profileName() + "!", 28, textColor);
        hello.setTypeface(Typeface.DEFAULT_BOLD);
        page.addView(hello);
        page.addView(space(8));
        page.addView(label(due > 0
                ? "Сегодня к повторению — " + due + " карточек."
                : "На сегодня всё повторено. Можно добавить новое слово.", 16, mutedColor));

        LinearLayout levelPanel = panel();
        LinearLayout levelRow = row();
        LinearLayout levelLeft = column();
        levelLeft.addView(label("Уровень " + level.level + " · " + level.title, 14, accent));
        levelLeft.addView(label(level.inLevel + " / " + level.needed + " XP", 25, textColor, true));
        levelRow.addView(levelLeft, new LinearLayout.LayoutParams(0, dp(70), 1));
        LinearLayout goal = column();
        goal.setGravity(Gravity.END);
        goal.addView(label("Цель дня", 13, mutedColor));
        goal.addView(label(store.reviewsToday() + " / 20", 22, textColor, true));
        levelRow.addView(goal, new LinearLayout.LayoutParams(dp(120), dp(70)));
        levelPanel.addView(levelRow);
        levelPanel.addView(progress(level.percent, accent));
        page.addView(space(12));
        page.addView(levelPanel);

        LinearLayout actions = row();
        Button review = primaryButton(due > 0 ? "Повторять" : "Открыть карточки",
                v -> navigate("cards"));
        actions.addView(review, weightedButton());
        actions.addView(secondaryButton("Словарь", v -> navigate("dictionary")),
                weightedButton());
        page.addView(space(14));
        page.addView(actions);

        LinearLayout metrics = new LinearLayout(this);
        metrics.setOrientation(LinearLayout.HORIZONTAL);
        metrics.setWeightSum(3);
        addMetric(metrics, String.valueOf(store.streak()), "дней подряд");
        addMetric(metrics, store.accuracy() + "%", "точность");
        addMetric(metrics, String.valueOf(store.learnedCount()), "слов в работе");
        page.addView(space(14));
        page.addView(metrics);

        DictionaryEntry word = repository.wordOfDay(
                java.util.Calendar.getInstance().get(java.util.Calendar.DAY_OF_YEAR));
        if (word != null) {
            LinearLayout wordPanel = panel();
            wordPanel.addView(label("Слово дня", 13, accent, true));
            wordPanel.addView(space(4));
            wordPanel.addView(label(word.front, 23, textColor, true));
            wordPanel.addView(label(word.back, 16, mutedColor));
            if (!word.example.isEmpty()) {
                wordPanel.addView(space(5));
                wordPanel.addView(label(word.example, 14, mutedColor));
            }
            Button add = secondaryButton("Добавить в карточки", v -> {
                boolean added = store.addCard(word.front, word.back, word.example);
                toast(added ? "Слово добавлено" : "Слово уже есть в карточках");
                render();
            });
            LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(-1, dp(44));
            p.topMargin = dp(12);
            wordPanel.addView(add, p);
            page.addView(space(14));
            page.addView(wordPanel);
        }
        content.addView(page);
    }

    private void renderCards() {
        topTitle.setText("Карточки");
        if (reviewingCard == null) {
            List<CardState> due = store.dueCards();
            reviewingCard = due.isEmpty() ? null : due.get(0);
            answerVisible = false;
        }

        LinearLayout page = page();
        if (reviewingCard == null) {
            LinearLayout empty = panel();
            empty.setGravity(Gravity.CENTER);
            empty.addView(label("Все карточки повторены", 22, textColor, true));
            empty.addView(space(8));
            empty.addView(label("Добавь новое слово или изучи словарь.", 15, mutedColor));
            page.addView(empty);
        } else {
            LinearLayout card = panel();
            card.setGravity(Gravity.CENTER);
            card.setPadding(dp(24), dp(30), dp(24), dp(24));
            card.addView(label("ПОВТОРЕНИЕ", 12, accent, true));
            card.addView(space(20));
            card.addView(label(reviewingCard.front, 32, textColor, true));
            card.addView(space(22));
            if (answerVisible) {
                card.addView(label(reviewingCard.back, 23, textColor, true));
                if (!reviewingCard.example.isEmpty()) {
                    card.addView(space(8));
                    card.addView(label(reviewingCard.example, 15, mutedColor));
                }
                card.addView(space(18));
                LinearLayout quality = row();
                quality.addView(qualityButton("Опять", 1, bad), weightedButton());
                quality.addView(qualityButton("Сложно", 3, Color.rgb(213, 145, 44)), weightedButton());
                quality.addView(qualityButton("Нормально", 4, accent), weightedButton());
                quality.addView(qualityButton("Легко", 5, good), weightedButton());
                card.addView(quality);
            } else {
                Button reveal = primaryButton("Показать ответ", v -> {
                    answerVisible = true;
                    render();
                });
                card.addView(reveal, new LinearLayout.LayoutParams(-1, dp(48)));
            }
            page.addView(card);
            page.addView(space(10));
            page.addView(label(reviewingCard.repetitions == 0
                    ? "Новое слово"
                    : "Серия: " + reviewingCard.repetitions + " · следующий интервал: "
                    + reviewingCard.intervalDays + " дн.", 13, mutedColor));
        }

        LinearLayout tools = row();
        tools.addView(secondaryButton("Добавить слово", v -> showAddCardDialog()), weightedButton());
        tools.addView(secondaryButton("Все слова", v -> navigate("dictionary")), weightedButton());
        page.addView(space(16));
        page.addView(tools);
        if (reviewingCard != null) {
            Button history = secondaryButton("История повторений", v -> showHistory(reviewingCard));
            LinearLayout.LayoutParams historyParams = new LinearLayout.LayoutParams(-1, dp(48));
            historyParams.topMargin = dp(8);
            page.addView(history, historyParams);
        }
        if (lastReviewedCard != null
                && (reviewingCard == null || !lastReviewedCard.front.equals(reviewingCard.front))) {
            Button lastHistory = secondaryButton(
                    "История: " + lastReviewedCard.front,
                    v -> showHistory(lastReviewedCard)
            );
            LinearLayout.LayoutParams historyParams = new LinearLayout.LayoutParams(-1, dp(48));
            historyParams.topMargin = dp(8);
            page.addView(lastHistory, historyParams);
        }
        content.addView(page);
    }

    private void renderDictionary() {
        topTitle.setText("Словарь");
        LinearLayout page = page();
        page.addView(label("340 слов для практики", 16, mutedColor));
        page.addView(space(12));
        dictionarySearch = new EditText(this);
        dictionarySearch.setSingleLine(true);
        dictionarySearch.setHint("Поиск по английскому или русскому");
        dictionarySearch.setTextColor(textColor);
        dictionarySearch.setHintTextColor(mutedColor);
        dictionarySearch.setBackground(round(surface, 12));
        dictionarySearch.setPadding(dp(14), 0, dp(14), 0);
        page.addView(dictionarySearch, new LinearLayout.LayoutParams(-1, dp(52)));
        dictionaryResults = new LinearLayout(this);
        dictionaryResults.setOrientation(LinearLayout.VERTICAL);
        page.addView(space(12));
        page.addView(dictionaryResults);
        dictionarySearch.addTextChangedListener(new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence s, int start, int count, int after) {
            }

            @Override
            public void onTextChanged(CharSequence s, int start, int before, int count) {
                renderDictionaryResults(s.toString());
            }

            @Override
            public void afterTextChanged(Editable s) {
            }
        });
        content.addView(page);
        renderDictionaryResults("");
    }

    private void renderDictionaryResults(String query) {
        if (dictionaryResults == null) return;
        dictionaryResults.removeAllViews();
        List<DictionaryEntry> entries = repository.search(query);
        if (entries.isEmpty()) {
            dictionaryResults.addView(label("Ничего не найдено", 16, mutedColor));
            return;
        }
        int limit = Math.min(entries.size(), query == null || query.isEmpty() ? 30 : 80);
        for (int i = 0; i < limit; i++) {
            DictionaryEntry entry = entries.get(i);
            LinearLayout item = panel();
            item.setOrientation(LinearLayout.HORIZONTAL);
            item.setGravity(Gravity.CENTER_VERTICAL);
            LinearLayout words = column();
            words.addView(label(entry.front, 17, textColor, true));
            words.addView(label(entry.back, 14, mutedColor));
            item.addView(words, new LinearLayout.LayoutParams(0, dp(58), 1));
            item.addView(label("＋", 24, accent, true));
            item.setOnClickListener(v -> showDictionaryEntry(entry));
            LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(-1, dp(70));
            p.bottomMargin = dp(8);
            dictionaryResults.addView(item, p);
        }
    }

    private void showHistory(CardState card) {
        LinearLayout box = column();
        box.setPadding(dp(18), dp(4), dp(18), dp(4));
        List<ReviewRecord> records = store.historyFor(card.front);
        if (records.isEmpty()) {
            box.addView(label("История пока пуста", 16, mutedColor));
        } else {
            SimpleDateFormat format = new SimpleDateFormat("dd.MM.yyyy HH:mm", Locale.US);
            for (ReviewRecord record : records) {
                LinearLayout item = column();
                item.setPadding(0, dp(4), 0, dp(10));
                item.addView(label(format.format(new Date(record.timestamp)), 14, textColor, true));
                item.addView(label(
                        record.successful() ? "Успешно · +10 XP" : "Ошибка · +2 XP",
                        14,
                        record.successful() ? good : bad
                ));
                box.addView(item);
            }
        }
        ScrollView scroll = new ScrollView(this);
        scroll.addView(box, new ViewGroup.LayoutParams(-1, -2));
        new AlertDialog.Builder(this)
                .setTitle("История: " + card.front)
                .setView(scroll)
                .setPositiveButton("Закрыть", null)
                .show();
    }

    private void showDictionaryEntry(DictionaryEntry entry) {
        LinearLayout box = column();
        box.setPadding(dp(20), dp(4), dp(4), 0);
        box.addView(label(entry.front, 22, textColor, true));
        box.addView(label(entry.back, 17, mutedColor));
        if (!entry.example.isEmpty()) {
            box.addView(space(8));
            box.addView(label(entry.example, 14, mutedColor));
        }
        AlertDialog dialog = new AlertDialog.Builder(this)
                .setTitle("Слово")
                .setView(box)
                .setNegativeButton("Закрыть", null)
                .setPositiveButton("В карточки", (d, which) -> {
                    boolean added = store.addCard(entry.front, entry.back, entry.example);
                    toast(added ? "Слово добавлено" : "Слово уже есть в карточках");
                })
                .create();
        dialog.show();
    }

    private void renderMore() {
        topTitle.setText("Ещё");
        LinearLayout page = page();
        page.addView(label("Практика", 24, textColor, true));
        page.addView(space(14));
        page.addView(menuCard("Грамматика", "27 упражнений с объяснением ошибок", "grammar"));
        page.addView(space(10));
        page.addView(menuCard("Чтение", "4 текста уровней A2–B2", "reading"));
        page.addView(space(10));
        page.addView(menuCard("Статистика", "XP, стрик, точность и прогресс", "stats"));
        page.addView(space(10));
        page.addView(menuCard("Настройки", "Профиль и локальное хранилище", "settings"));
        content.addView(page);
    }

    private LinearLayout menuCard(String title, String subtitle, String target) {
        LinearLayout card = panel();
        card.setOnClickListener(v -> {
            selectedTab = "more";
            screen = target;
            render();
        });
        card.addView(label(title, 18, textColor, true));
        card.addView(space(3));
        card.addView(label(subtitle, 14, mutedColor));
        return card;
    }

    private void renderGrammar() {
        topTitle.setText("Грамматика");
        List<GrammarEntry> entries = repository.grammar();
        LinearLayout page = page();
        if (entries.isEmpty()) {
            page.addView(label("Упражнения не загрузились", 18, textColor, true));
            content.addView(page);
            return;
        }
        GrammarEntry entry = entries.get(grammarIndex % entries.size());
        LinearLayout card = panel();
        card.addView(label("Упражнение " + (grammarIndex % entries.size() + 1) + " из " + entries.size(),
                13, accent, true));
        card.addView(space(8));
        card.addView(label(entry.topic, 15, mutedColor, true));
        card.addView(space(16));
        card.addView(label(entry.prompt, 23, textColor, true));
        card.addView(space(18));
        for (int i = 0; i < entry.options.size(); i++) {
            int choice = i;
            Button option = secondaryButton(entry.options.get(i), v -> answerGrammar(choice));
            if (grammarAnswered) {
                if (choice == entry.correctIndex) option.setBackground(round(good, 12));
                else if (choice == grammarChoice) option.setBackground(round(bad, 12));
            }
            LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(-1, dp(48));
            p.bottomMargin = dp(8);
            card.addView(option, p);
        }
        if (grammarAnswered) {
            boolean correct = grammarChoice == entry.correctIndex;
            card.addView(space(8));
            card.addView(label(correct ? "Верно! +15 XP" : "Ошибка. Правильный ответ: "
                    + entry.options.get(entry.correctIndex), 16, correct ? good : bad, true));
            if (!entry.explanation.isEmpty()) {
                card.addView(space(6));
                card.addView(label(entry.explanation, 14, mutedColor));
            }
            Button next = primaryButton("Следующее упражнение", v -> {
                grammarIndex = (grammarIndex + 1) % entries.size();
                grammarAnswered = false;
                grammarChoice = -1;
                render();
            });
            LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(-1, dp(48));
            p.topMargin = dp(12);
            card.addView(next, p);
        }
        page.addView(card);
        content.addView(page);
    }

    private void answerGrammar(int choice) {
        if (grammarAnswered) return;
        List<GrammarEntry> entries = repository.grammar();
        if (entries.isEmpty()) return;
        GrammarEntry entry = entries.get(grammarIndex % entries.size());
        grammarChoice = choice;
        grammarAnswered = true;
        store.recordGrammar(choice == entry.correctIndex);
        render();
    }

    private void renderReadingList() {
        topTitle.setText("Чтение");
        LinearLayout page = page();
        page.addView(label("Выбери текст для практики", 16, mutedColor));
        page.addView(space(14));
        for (ReadingText text : repository.texts()) {
            LinearLayout card = panel();
            card.setOnClickListener(v -> {
                selectedText = text;
                screen = "reading-detail";
                render();
            });
            LinearLayout row = row();
            LinearLayout words = column();
            words.addView(label(text.title, 18, textColor, true));
            words.addView(label(text.level + " · " + text.summary, 14, mutedColor));
            row.addView(words, new LinearLayout.LayoutParams(0, dp(58), 1));
            row.addView(label("→", 24, accent, true));
            card.addView(row);
            LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(-1, dp(86));
            p.bottomMargin = dp(10);
            page.addView(card, p);
        }
        if (repository.texts().isEmpty()) {
            page.addView(label("Тексты не загрузились", 16, mutedColor));
        }
        content.addView(page);
    }

    private void renderReadingDetail() {
        topTitle.setText(selectedText == null ? "Чтение" : selectedText.title);
        LinearLayout page = page();
        if (selectedText != null) {
            page.addView(label(selectedText.level, 13, accent, true));
            page.addView(space(8));
            page.addView(label(selectedText.summary, 15, mutedColor));
            page.addView(space(16));
            for (String paragraph : selectedText.content.split("\\n\\n")) {
                TextView paragraphView = label(paragraph.trim(), 17, textColor);
                paragraphView.setLineSpacing(3, 1.08f);
                page.addView(paragraphView);
                page.addView(space(15));
            }
        }
        page.addView(secondaryButton("К списку текстов", v -> {
            screen = "reading";
            render();
        }));
        content.addView(page);
    }

    private void renderStats() {
        topTitle.setText("Статистика");
        LinearLayout page = page();
        MobileStore.LevelInfo level = store.level();
        page.addView(label("Твой прогресс", 24, textColor, true));
        page.addView(space(16));
        addStat(page, "XP", String.valueOf(store.xp()));
        addStat(page, "Уровень", level.level + " · " + level.title);
        addStat(page, "Карточки", store.cards().size() + " всего · " + store.learnedCount() + " в работе");
        addStat(page, "Повторения", store.reviews() + " · точность " + store.accuracy() + "%");
        addStat(page, "Грамматика", store.grammarCorrect() + " / " + store.grammarTotal());
        addStat(page, "Серия", store.streak() + " дн.");
        page.addView(space(18));
        page.addView(secondaryButton("Сбросить локальный прогресс", v -> confirmReset()));
        content.addView(page);
    }

    private void addStat(LinearLayout page, String label, String value) {
        LinearLayout row = panel();
        row.setGravity(Gravity.CENTER_VERTICAL);
        row.addView(label(label, 15, mutedColor), new LinearLayout.LayoutParams(0, dp(42), 1));
        row.addView(label(value, 15, textColor, true));
        LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(-1, dp(58));
        p.bottomMargin = dp(8);
        page.addView(row, p);
    }

    private void renderSettings() {
        topTitle.setText("Настройки");
        LinearLayout page = page();
        LinearLayout profilePanel = panel();
        profilePanel.addView(label("Профиль", 14, mutedColor));
        profilePanel.addView(space(5));
        profilePanel.addView(label(store.profileName(), 24, textColor, true));
        profilePanel.addView(space(10));
        profilePanel.addView(secondaryButton("Изменить имя", v -> showProfile()));
        page.addView(profilePanel);
        page.addView(space(12));
        LinearLayout about = panel();
        about.addView(label("О приложении", 17, textColor, true));
        about.addView(space(6));
        about.addView(label("LinguaRust Mobile — офлайн-версия изучающей площадки. "
                + "Словарь, тексты и упражнения берутся из общей папки data проекта.", 14, mutedColor));
        page.addView(about);
        content.addView(page);
    }

    private void showProfile() {
        EditText input = new EditText(this);
        input.setSingleLine(true);
        input.setHint("Имя");
        input.setText(store.profileName());
        input.setTextColor(textColor);
        input.setHintTextColor(mutedColor);
        input.setPadding(dp(14), 0, dp(14), 0);
        input.setBackground(round(surface, 12));
        FrameLayout holder = new FrameLayout(this);
        holder.setPadding(dp(20), dp(4), dp(20), 0);
        holder.addView(input, new FrameLayout.LayoutParams(-1, dp(52)));
        new AlertDialog.Builder(this)
                .setTitle("Профиль")
                .setView(holder)
                .setNegativeButton("Отмена", null)
                .setPositiveButton("Сохранить", (dialog, which) -> {
                    store.setProfileName(input.getText().toString());
                    render();
                })
                .show();
    }

    private void showAddCardDialog() {
        LinearLayout form = column();
        form.setPadding(dp(20), dp(4), dp(20), 0);
        EditText front = field("Слово по-английски");
        EditText back = field("Перевод");
        EditText example = field("Пример (необязательно)");
        form.addView(front);
        form.addView(space(8));
        form.addView(back);
        form.addView(space(8));
        form.addView(example);
        new AlertDialog.Builder(this)
                .setTitle("Новая карточка")
                .setView(form)
                .setNegativeButton("Отмена", null)
                .setPositiveButton("Добавить", (dialog, which) -> {
                    boolean added = store.addCard(front.getText().toString(),
                            back.getText().toString(), example.getText().toString());
                    toast(added ? "Карточка добавлена" : "Нужны слово и перевод");
                    render();
                })
                .show();
    }

    private void confirmReset() {
        new AlertDialog.Builder(this)
                .setTitle("Сбросить прогресс?")
                .setMessage("Карточки, XP и статистика этого профиля будут удалены.")
                .setNegativeButton("Отмена", null)
                .setPositiveButton("Сбросить", (dialog, which) -> {
                    store.resetProgress();
                    reviewingCard = null;
                    lastReviewedCard = null;
                    answerVisible = false;
                    toast("Прогресс сброшен");
                    render();
                })
                .show();
    }

    private Button qualityButton(String title, int quality, int color) {
        return button(title, v -> {
            if (reviewingCard == null) return;
            lastReviewedCard = reviewingCard;
            store.review(reviewingCard, quality);
            reviewingCard = null;
            answerVisible = false;
            toast(quality >= 3 ? "+10 XP" : "+2 XP");
            render();
        }, color, Color.WHITE);
    }

    private EditText field(String hint) {
        EditText field = new EditText(this);
        field.setSingleLine(true);
        field.setHint(hint);
        field.setTextColor(textColor);
        field.setHintTextColor(mutedColor);
        field.setPadding(dp(14), 0, dp(14), 0);
        field.setBackground(round(surface, 12));
        return field;
    }

    private LinearLayout page() {
        LinearLayout page = new LinearLayout(this);
        page.setOrientation(LinearLayout.VERTICAL);
        return page;
    }

    private LinearLayout panel() {
        LinearLayout panel = new LinearLayout(this);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setPadding(dp(18), dp(16), dp(18), dp(16));
        panel.setBackground(round(surface, 16));
        return panel;
    }

    private LinearLayout column() {
        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        return column;
    }

    private LinearLayout row() {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(Gravity.CENTER_VERTICAL);
        return row;
    }

    private TextView label(String value, float size, int color) {
        return label(value, size, color, false);
    }

    private TextView label(String value, float size, int color, boolean bold) {
        TextView view = new TextView(this);
        view.setText(value);
        view.setTextSize(TypedValue.COMPLEX_UNIT_SP, size);
        view.setTextColor(color);
        if (bold) view.setTypeface(Typeface.DEFAULT_BOLD);
        return view;
    }

    private View space(int height) {
        View view = new View(this);
        view.setLayoutParams(new LinearLayout.LayoutParams(1, height));
        return view;
    }

    private Button primaryButton(String title, View.OnClickListener listener) {
        return button(title, listener, accent, Color.WHITE);
    }

    private Button secondaryButton(String title, View.OnClickListener listener) {
        return button(title, listener, surfaceMuted, textColor);
    }

    private Button button(String title, View.OnClickListener listener) {
        return button(title, listener, surfaceMuted, textColor);
    }

    private Button button(String title, View.OnClickListener listener, int backgroundColor, int foreground) {
        Button button = new Button(this);
        button.setText(title);
        button.setAllCaps(false);
        button.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14);
        button.setTextColor(foreground);
        button.setGravity(Gravity.CENTER);
        button.setPadding(dp(8), 0, dp(8), 0);
        button.setBackground(round(backgroundColor, 12));
        button.setOnClickListener(listener);
        return button;
    }

    private GradientDrawable round(int fill, int radiusDp) {
        GradientDrawable drawable = new GradientDrawable();
        drawable.setColor(fill);
        drawable.setCornerRadius(dp(radiusDp));
        if (fill != Color.TRANSPARENT) {
            drawable.setStroke(dp(1), border);
        }
        return drawable;
    }

    private View progress(int percent, int color) {
        FrameLayout frame = new FrameLayout(this);
        frame.setBackground(round(surfaceMuted, 8));
        View fill = new View(this);
        FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(
                Math.max(dp(3), getResources().getDisplayMetrics().widthPixels * percent / 100), dp(8));
        fill.setBackground(round(color, 8));
        frame.addView(fill, params);
        LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(-1, dp(8));
        p.topMargin = dp(10);
        frame.setLayoutParams(p);
        return frame;
    }

    private LinearLayout.LayoutParams weightedButton() {
        return new LinearLayout.LayoutParams(0, dp(48), 1);
    }

    private void addMetric(LinearLayout parent, String value, String caption) {
        LinearLayout metric = column();
        metric.setGravity(Gravity.CENTER);
        metric.setBackground(round(surface, 14));
        metric.addView(label(value, 24, accent, true));
        metric.addView(label(caption, 12, mutedColor));
        parent.addView(metric, new LinearLayout.LayoutParams(0, dp(84), 1));
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    private void toast(String message) {
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show();
    }
}
