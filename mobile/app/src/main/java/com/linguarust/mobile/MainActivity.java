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
import android.os.CountDownTimer;
import android.text.Editable;
import android.text.InputType;
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
import java.util.ArrayList;
import java.util.Collections;
import java.util.Date;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Random;
import java.util.Set;

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
    private String cardFilter = "all";
    private EditText cardSearch;
    private LinearLayout cardResults;
    private int grammarIndex;
    private boolean grammarAnswered;
    private int grammarChoice = -1;
    private ReadingText selectedText;
    private DictionaryEntry typingEntry;
    private EditText typingInput;
    private boolean typingAnswered;
    private boolean typingCorrect;
    private int typingAnsweredCount;
    private int typingCorrectCount;
    private final Random typingRandom = new Random();
    private final List<ExamQuestion> examQuestions = new ArrayList<>();
    private final Random examRandom = new Random();
    private int examIndex;
    private int examCorrect;
    private int examAnsweredCount;
    private int examChoice = -1;
    private boolean examAnswered;
    private CountDownTimer examTimer;
    private TextView examTimerView;

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

    @Override
    protected void onDestroy() {
        if (examTimer != null) examTimer.cancel();
        super.onDestroy();
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
        button.setTag(key);
        button.setAllCaps(false);
        button.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
        button.setPadding(2, 2, 2, 2);
        parent.addView(button, new LinearLayout.LayoutParams(0, dp(52), 1));
    }

    private void navigate(String key) {
        selectedTab = "library".equals(key) ? "cards" : key;
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
        if ("exam".equals(key)) {
            startExam();
        } else if ("typing".equals(key)) {
            startTyping();
        } else if (examTimer != null) {
            examTimer.cancel();
            examTimer = null;
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
            case "library":
                renderCardLibrary();
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
            case "exam":
                renderExam();
                break;
            case "exam-result":
                renderExamResult();
                break;
            case "typing":
                renderTyping();
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
            String key = String.valueOf(nav.getTag());
            boolean active = key.equals(selectedTab);
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
        goal.addView(label(store.reviewsToday() + " / " + store.goal(), 22, textColor, true));
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
        topTitle.setText(t("Карточки"));
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
        tools.addView(secondaryButton("Все слова", v -> navigate("library")), weightedButton());
        page.addView(space(16));
        page.addView(tools);
        if (reviewingCard != null) {
            Button history = secondaryButton("История повторений", v -> showHistory(reviewingCard));
            LinearLayout.LayoutParams historyParams = new LinearLayout.LayoutParams(-1, dp(48));
            historyParams.topMargin = dp(8);
            page.addView(history, historyParams);
        }
        Button compare = secondaryButton("Сравнить карточки", v -> chooseCompareCards());
        LinearLayout.LayoutParams compareParams = new LinearLayout.LayoutParams(-1, dp(48));
        compareParams.topMargin = dp(8);
        page.addView(compare, compareParams);
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

    private void renderCardLibrary() {
        topTitle.setText(t("Все карточки"));
        LinearLayout page = page();
        page.addView(label(store.cards().size() + " карточек в колоде", 16, mutedColor));
        page.addView(space(12));

        cardSearch = new EditText(this);
        cardSearch.setSingleLine(true);
        cardSearch.setHint(t("Поиск по карточкам"));
        cardSearch.setTextColor(textColor);
        cardSearch.setHintTextColor(mutedColor);
        cardSearch.setBackground(round(surface, 12));
        cardSearch.setPadding(dp(14), 0, dp(14), 0);
        page.addView(cardSearch, new LinearLayout.LayoutParams(-1, dp(52)));
        cardSearch.addTextChangedListener(new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence s, int start, int count, int after) {
            }

            @Override
            public void onTextChanged(CharSequence s, int start, int before, int count) {
                renderCardResults(s.toString());
            }

            @Override
            public void afterTextChanged(Editable s) {
            }
        });

        LinearLayout filters = row();
        addFilterButton(filters, "Все", "all");
        addFilterButton(filters, "К повторению", "due");
        addFilterButton(filters, "Новые", "new");
        addFilterButton(filters, "В работе", "progress");
        page.addView(space(10));
        page.addView(filters);
        cardResults = new LinearLayout(this);
        cardResults.setOrientation(LinearLayout.VERTICAL);
        page.addView(space(12));
        page.addView(cardResults);
        content.addView(page);
        renderCardResults("");
    }

    private void addFilterButton(LinearLayout parent, String title, String filter) {
        Button button = secondaryButton(title, v -> {
            cardFilter = filter;
            render();
        });
        if (cardFilter.equals(filter)) {
            button.setBackground(round(accent, 12));
            button.setTextColor(Color.WHITE);
        }
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(0, dp(42), 1);
        params.rightMargin = dp(4);
        parent.addView(button, params);
    }

    private void renderCardResults(String query) {
        if (cardResults == null) return;
        cardResults.removeAllViews();
        String needle = query == null ? "" : query.trim().toLowerCase(Locale.ROOT);
        long now = System.currentTimeMillis();
        int shown = 0;
        for (CardState card : store.cards()) {
            boolean matches = needle.isEmpty()
                    || card.front.toLowerCase(Locale.ROOT).contains(needle)
                    || card.back.toLowerCase(Locale.ROOT).contains(needle);
            boolean filter = "all".equals(cardFilter)
                    || ("due".equals(cardFilter) && card.isDue(now))
                    || ("new".equals(cardFilter) && card.repetitions == 0)
                    || ("progress".equals(cardFilter) && card.repetitions > 0);
            if (!matches || !filter) continue;
            if (shown++ >= 100) break;

            LinearLayout item = panel();
            item.setOnClickListener(v -> showCardDetails(card));
            item.addView(label(card.front, 17, textColor, true));
            item.addView(label(card.back, 14, mutedColor));
            String status = card.repetitions == 0
                    ? "Новое"
                    : (card.isDue(now) ? "К повторению" : "В работе");
            item.addView(label(status + " · " + card.repetitions + " повторений", 12, accent));
            LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(-1, dp(92));
            params.bottomMargin = dp(8);
            cardResults.addView(item, params);
        }
        if (shown == 0) cardResults.addView(label("Карточки не найдены", 16, mutedColor));
    }

    private void showCardDetails(CardState card) {
        LinearLayout box = column();
        box.setPadding(dp(20), dp(4), dp(20), dp(4));
        box.addView(label(card.front, 24, textColor, true));
        box.addView(label(card.back, 17, mutedColor));
        if (!card.example.isEmpty()) {
            box.addView(space(8));
            box.addView(label(card.example, 14, mutedColor));
        }
        box.addView(space(10));
        box.addView(label("Повторения: " + card.repetitions
                + " · освоение: " + card.masteryPercent() + "%", 14, accent));
        new AlertDialog.Builder(this)
                .setTitle(t("Карточка"))
                .setView(box)
                .setNegativeButton(t("Удалить"), (dialog, which) -> confirmDelete(card))
                .setNeutralButton(t("История"), (dialog, which) -> showHistory(card))
                .setPositiveButton(t("Повторить"), (dialog, which) -> {
                    reviewingCard = card;
                    answerVisible = false;
                    selectedTab = "cards";
                    screen = "cards";
                    render();
                })
                .show();
    }

    private void confirmDelete(CardState card) {
        new AlertDialog.Builder(this)
                .setTitle(t("Удалить карточку?"))
                .setMessage(t("История повторений останется в статистике."))
                .setNegativeButton(t("Отмена"), null)
                .setPositiveButton(t("Удалить"), (dialog, which) -> {
                    store.deleteCard(card.front);
                    toast("Карточка удалена");
                    render();
                })
                .show();
    }

    private void renderDictionary() {
        topTitle.setText(t("Словарь"));
        LinearLayout page = page();
        page.addView(label("340 слов для практики", 16, mutedColor));
        page.addView(space(12));
        dictionarySearch = new EditText(this);
        dictionarySearch.setSingleLine(true);
        dictionarySearch.setHint(t("Поиск по английскому или русскому"));
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

    private interface CardChoiceListener {
        void onChosen(CardState card);
    }

    private void chooseCompareCards() {
        List<CardState> all = store.cards();
        if (all.size() < 2) {
            new AlertDialog.Builder(this)
                    .setTitle(t("Сравнение карточек"))
                    .setMessage(t("Для сравнения добавь хотя бы две карточки."))
                    .setPositiveButton(t("Закрыть"), null)
                    .show();
            return;
        }
        chooseCompareCard("Выбери первую карточку", first -> chooseCompareCard(
                "Выбери вторую карточку",
                second -> {
                    if (!first.front.equalsIgnoreCase(second.front)) {
                        showComparison(first, second);
                    } else {
                        toast("Выбери разные карточки");
                    }
                }
        ));
    }

    private void chooseCompareCard(String title, CardChoiceListener listener) {
        List<CardState> all = store.cards();
        String[] items = new String[all.size()];
        for (int i = 0; i < all.size(); i++) items[i] = all.get(i).front;
        new AlertDialog.Builder(this)
                .setTitle(t(title))
                .setSingleChoiceItems(items, -1, (dialog, which) -> {
                    dialog.dismiss();
                    listener.onChosen(all.get(which));
                })
                .setNegativeButton(t("Отмена"), null)
                .show();
    }

    private void showComparison(CardState left, CardState right) {
        LinearLayout box = column();
        box.setPadding(dp(18), dp(4), dp(18), dp(4));
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        addComparisonColumn(row, left, "Карточка 1");
        addComparisonColumn(row, right, "Карточка 2");
        box.addView(row);
        ScrollView scroll = new ScrollView(this);
        scroll.addView(box, new ViewGroup.LayoutParams(-1, -2));
        new AlertDialog.Builder(this)
                .setTitle(t("Сравнение карточек"))
                .setView(scroll)
                .setPositiveButton(t("Закрыть"), null)
                .show();
    }

    private void addComparisonColumn(LinearLayout row, CardState card, String caption) {
        LinearLayout column = panel();
        column.addView(label(caption, 13, accent, true));
        column.addView(space(6));
        column.addView(label(card.front, 20, textColor, true));
        column.addView(space(5));
        column.addView(label(card.back, 15, mutedColor));
        if (!card.example.isEmpty()) {
            column.addView(space(5));
            column.addView(label(card.example, 13, mutedColor));
        }
        column.addView(space(9));
        column.addView(label("Повторения: " + card.repetitions, 13, textColor));
        column.addView(label("Интервал: " + card.intervalDays + " дн.", 13, mutedColor));
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(0, -2, 1);
        params.rightMargin = dp(5);
        row.addView(column, params);
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
                .setTitle(t("История: " + card.front))
                .setView(scroll)
                .setPositiveButton(t("Закрыть"), null)
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
                .setTitle(t("Слово"))
                .setView(box)
                .setNegativeButton(t("Закрыть"), null)
                .setPositiveButton(t("В карточки"), (d, which) -> {
                    boolean added = store.addCard(entry.front, entry.back, entry.example);
                    toast(added ? "Слово добавлено" : "Слово уже есть в карточках");
                })
                .create();
        dialog.show();
    }

    private void renderMore() {
        topTitle.setText(t("Ещё"));
        LinearLayout page = page();
        page.addView(label("Практика", 24, textColor, true));
        page.addView(space(14));
        page.addView(menuCard("Грамматика", "27 упражнений с объяснением ошибок", "grammar"));
        page.addView(space(10));
        page.addView(menuCard("Чтение", "4 текста уровней A2–B2", "reading"));
        page.addView(space(10));
        page.addView(menuCard("Статистика", "XP, стрик, точность и прогресс", "stats"));
        page.addView(space(10));
        page.addView(menuCard("Экзамен", "10 вопросов за 90 секунд", "exam"));
        page.addView(space(10));
        page.addView(menuCard("Тренажёр письма", "Перевод → слово с клавиатуры", "typing"));
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
        topTitle.setText(t("Грамматика"));
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
        topTitle.setText(t("Чтение"));
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
        topTitle.setText(t(selectedText == null ? "Чтение" : selectedText.title));
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

    private void startTyping() {
        typingAnswered = false;
        typingCorrect = false;
        typingAnsweredCount = 0;
        typingCorrectCount = 0;
        typingInput = null;
        typingEntry = randomTypingEntry();
    }

    private DictionaryEntry randomTypingEntry() {
        if (repository.dictionary().isEmpty()) return null;
        DictionaryEntry candidate;
        do {
            candidate = repository.dictionary().get(typingRandom.nextInt(repository.dictionary().size()));
        } while (typingEntry != null
                && repository.dictionary().size() > 1
                && candidate.front.equalsIgnoreCase(typingEntry.front));
        return candidate;
    }

    private void renderTyping() {
        topTitle.setText(t("Тренажёр письма"));
        if (typingEntry == null) {
            LinearLayout page = page();
            page.addView(label("Словарь не загрузился", 18, textColor, true));
            content.addView(page);
            return;
        }

        LinearLayout page = page();
        LinearLayout meta = row();
        meta.addView(label("Правильных: " + typingCorrectCount, 14, mutedColor),
                new LinearLayout.LayoutParams(0, dp(32), 1));
        meta.addView(label("Проверок: " + typingAnsweredCount, 14, accent));
        page.addView(meta);
        page.addView(space(12));

        LinearLayout card = panel();
        card.addView(label("Перевод", 13, accent, true));
        card.addView(space(10));
        card.addView(label(typingEntry.back, 27, textColor, true));
        card.addView(space(6));
        card.addView(label("Введи английское слово", 14, mutedColor));
        card.addView(space(14));

        typingInput = new EditText(this);
        typingInput.setSingleLine(true);
        typingInput.setHint(t("Английское слово"));
        typingInput.setTextColor(textColor);
        typingInput.setHintTextColor(mutedColor);
        typingInput.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_CAP_SENTENCES);
        typingInput.setBackground(round(surfaceMuted, 12));
        typingInput.setPadding(dp(14), 0, dp(14), 0);
        typingInput.setEnabled(!typingAnswered);
        card.addView(typingInput, new LinearLayout.LayoutParams(-1, dp(52)));

        if (!typingAnswered) {
            Button check = primaryButton("Проверить", v -> checkTyping());
            LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(-1, dp(48));
            params.topMargin = dp(12);
            card.addView(check, params);
        } else {
            card.addView(space(12));
            card.addView(label(typingCorrect ? "Верно! +6 XP" : "Ошибка. +1 XP", 16,
                    typingCorrect ? good : bad, true));
            card.addView(label("Правильный ответ: " + typingEntry.front, 16, textColor, true));
            if (!typingEntry.example.isEmpty()) {
                card.addView(space(6));
                card.addView(label(typingEntry.example, 14, mutedColor));
            }
            Button next = primaryButton("Следующее слово", v -> {
                typingEntry = randomTypingEntry();
                typingAnswered = false;
                typingCorrect = false;
                typingInput = null;
                render();
            });
            LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(-1, dp(48));
            params.topMargin = dp(12);
            card.addView(next, params);
        }
        page.addView(card);
        content.addView(page);
    }

    private void checkTyping() {
        if (typingAnswered || typingEntry == null || typingInput == null) return;
        String answer = normalizeTyping(typingInput.getText().toString());
        String expected = normalizeTyping(typingEntry.front);
        typingCorrect = !answer.isEmpty() && answer.equals(expected);
        typingAnswered = true;
        typingAnsweredCount++;
        if (typingCorrect) typingCorrectCount++;
        store.recordTyping(typingCorrect);
        render();
    }

    private String normalizeTyping(String value) {
        String result = value == null ? "" : value.trim().toLowerCase(Locale.ROOT)
                .replaceAll("\\s+", " ");
        if (result.startsWith("to ")) result = result.substring(3);
        return result;
    }

    private void startExam() {
        examQuestions.clear();
        examIndex = 0;
        examCorrect = 0;
        examChoice = -1;
        examAnswered = false;
        examAnsweredCount = 0;

        List<DictionaryEntry> pool = new ArrayList<>(repository.dictionary());
        if (pool.size() < 4) return;
        for (int i = 0; i < 10 && !pool.isEmpty(); i++) {
            DictionaryEntry entry = pool.remove(examRandom.nextInt(pool.size()));
            boolean ruToEn = i % 2 == 0;
            List<String> options = new ArrayList<>();
            Set<String> used = new HashSet<>();
            String correct = ruToEn ? entry.front : entry.back;
            options.add(correct);
            used.add(correct);
            while (options.size() < 4 && !pool.isEmpty()) {
                DictionaryEntry candidate = pool.get(examRandom.nextInt(pool.size()));
                String option = ruToEn ? candidate.front : candidate.back;
                if (used.add(option)) options.add(option);
            }
            while (options.size() < 4) options.add("—");
            Collections.shuffle(options, examRandom);
            examQuestions.add(new ExamQuestion(
                    entry,
                    ruToEn,
                    options,
                    options.indexOf(correct)
            ));
        }

        if (examTimer != null) examTimer.cancel();
        examTimer = new CountDownTimer(90_000L, 1_000L) {
            @Override
            public void onTick(long millisUntilFinished) {
                if (examTimerView != null) {
                    long seconds = millisUntilFinished / 1000L;
                    examTimerView.setText(String.format(
                            Locale.US, "%02d:%02d", seconds / 60, seconds % 60
                    ));
                }
            }

            @Override
            public void onFinish() {
                if (screen.equals("exam")) finishExam();
            }
        }.start();
    }

    private void renderExam() {
        topTitle.setText(t("Экзамен"));
        if (examQuestions.isEmpty()) {
            LinearLayout page = page();
            page.addView(label("Недостаточно слов для экзамена", 18, textColor, true));
            content.addView(page);
            return;
        }
        if (examIndex >= examQuestions.size()) {
            renderExamResult();
            return;
        }

        ExamQuestion question = examQuestions.get(examIndex);
        LinearLayout page = page();
        LinearLayout meta = row();
        meta.addView(label("Вопрос " + (examIndex + 1) + " из " + examQuestions.size(),
                14, mutedColor), new LinearLayout.LayoutParams(0, dp(32), 1));
        examTimerView = label("01:30", 18, accent, true);
        meta.addView(examTimerView);
        page.addView(meta);
        page.addView(space(10));

        LinearLayout card = panel();
        card.addView(label(question.ruToEn ? "Выбери слово" : "Выбери перевод", 13, accent, true));
        card.addView(space(10));
        card.addView(label(question.prompt(), 27, textColor, true));
        if (question.ruToEn && !question.entry.example.isEmpty()) {
            card.addView(space(6));
            card.addView(label(question.entry.example, 14, mutedColor));
        }
        card.addView(space(18));
        for (int i = 0; i < question.options.size(); i++) {
            int choice = i;
            Button option = secondaryButton(question.options.get(i), v -> answerExam(choice));
            if (examAnswered) {
                if (choice == question.correctIndex) option.setBackground(round(good, 12));
                else if (choice == examChoice) option.setBackground(round(bad, 12));
            }
            LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(-1, dp(50));
            params.bottomMargin = dp(8);
            card.addView(option, params);
        }
        if (examAnswered) {
            boolean correct = examChoice == question.correctIndex;
            card.addView(space(6));
            card.addView(label(correct ? "Верно! +8 XP" : "Ошибка. +2 XP", 15,
                    correct ? good : bad, true));
            Button next = primaryButton(
                    examIndex + 1 == examQuestions.size() ? "Завершить экзамен" : "Следующий вопрос",
                    v -> {
                        if (examIndex + 1 >= examQuestions.size()) finishExam();
                        else {
                            examIndex++;
                            examChoice = -1;
                            examAnswered = false;
                            render();
                        }
                    }
            );
            LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(-1, dp(48));
            params.topMargin = dp(10);
            card.addView(next, params);
        }
        page.addView(card);
        content.addView(page);
    }

    private void answerExam(int choice) {
        if (examAnswered || examIndex >= examQuestions.size()) return;
        ExamQuestion question = examQuestions.get(examIndex);
        examChoice = choice;
        examAnswered = true;
        examAnsweredCount++;
        boolean correct = choice == question.correctIndex;
        if (correct) examCorrect++;
        store.recordExam(correct);
        render();
    }

    private void finishExam() {
        if (examTimer != null) {
            examTimer.cancel();
            examTimer = null;
        }
        screen = "exam-result";
        render();
    }

    private void renderExamResult() {
        topTitle.setText(t("Результат экзамена"));
        LinearLayout page = page();
        LinearLayout result = panel();
        result.setGravity(Gravity.CENTER);
        result.addView(label("Экзамен завершён", 23, textColor, true));
        result.addView(space(10));
        result.addView(label(examCorrect + " / " + examAnsweredCount, 38, accent, true));
        result.addView(label("правильных ответов", 15, mutedColor));
        result.addView(space(8));
        int earnedXp = examCorrect * 8 + Math.max(0, examAnsweredCount - examCorrect) * 2;
        result.addView(label("+" + earnedXp + " XP", 18, good, true));
        page.addView(result);
        page.addView(space(16));
        page.addView(primaryButton("Пройти ещё раз", v -> {
            screen = "exam";
            startExam();
            render();
        }));
        page.addView(secondaryButton("В меню", v -> navigate("more")));
        content.addView(page);
    }

    private void renderStats() {
        topTitle.setText(t("Статистика"));
        LinearLayout page = page();
        MobileStore.LevelInfo level = store.level();
        page.addView(label("Твой прогресс", 24, textColor, true));
        page.addView(space(16));
        addStat(page, "XP", String.valueOf(store.xp()));
        addStat(page, "Уровень", level.level + " · " + level.title);
        addStat(page, "Карточки", store.cards().size() + " всего · " + store.learnedCount() + " в работе");
        addStat(page, "Повторения", store.reviews() + " · точность " + store.accuracy() + "%");
        addStat(page, "Грамматика", store.grammarCorrect() + " / " + store.grammarTotal());
        addStat(page, "Экзамен", store.examCorrect() + " / " + store.examTotal());
        addStat(page, "Тренажёр письма", store.typingCorrect() + " / " + store.typingTotal());
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
        topTitle.setText(t("Настройки"));
        LinearLayout page = page();
        LinearLayout profilePanel = panel();
        profilePanel.addView(label("Профиль", 14, mutedColor));
        profilePanel.addView(space(5));
        profilePanel.addView(label(store.profileName(), 24, textColor, true));
        profilePanel.addView(space(10));
        profilePanel.addView(secondaryButton("Изменить имя", v -> showProfile()));
        page.addView(profilePanel);
        page.addView(space(12));

        LinearLayout languagePanel = panel();
        languagePanel.addView(label("Язык интерфейса", 14, mutedColor));
        languagePanel.addView(space(5));
        languagePanel.addView(label(store.english() ? "English" : "Русский", 24, textColor, true));
        languagePanel.addView(space(10));
        languagePanel.addView(secondaryButton(
                store.english() ? "Переключить на русский" : "Переключить на английский",
                v -> {
                    store.setEnglish(!store.english());
                    toast(store.english() ? "English interface" : "Русский интерфейс");
                    render();
                }
        ));
        page.addView(languagePanel);
        page.addView(space(12));

        LinearLayout goalPanel = panel();
        goalPanel.addView(label("Дневная цель", 14, mutedColor));
        goalPanel.addView(space(5));
        goalPanel.addView(label(store.goal() + " повторений в день", 20, textColor, true));
        goalPanel.addView(space(10));
        LinearLayout goalRow = row();
        for (int value : new int[]{10, 20, 50}) {
            Button goalButton = secondaryButton(String.valueOf(value), v -> {
                store.setGoal(value);
                render();
            });
            if (store.goal() == value) {
                goalButton.setBackground(round(accent, 12));
                goalButton.setTextColor(Color.WHITE);
            }
            goalRow.addView(goalButton, weightedButton());
        }
        goalPanel.addView(goalRow);
        page.addView(goalPanel);
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
        input.setHint(t("Имя"));
        input.setText(store.profileName());
        input.setTextColor(textColor);
        input.setHintTextColor(mutedColor);
        input.setPadding(dp(14), 0, dp(14), 0);
        input.setBackground(round(surface, 12));
        FrameLayout holder = new FrameLayout(this);
        holder.setPadding(dp(20), dp(4), dp(20), 0);
        holder.addView(input, new FrameLayout.LayoutParams(-1, dp(52)));
        new AlertDialog.Builder(this)
                .setTitle(t("Профиль"))
                .setView(holder)
                .setNegativeButton(t("Отмена"), null)
                .setPositiveButton(t("Сохранить"), (dialog, which) -> {
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
                .setTitle(t("Новая карточка"))
                .setView(form)
                .setNegativeButton(t("Отмена"), null)
                .setPositiveButton(t("Добавить"), (dialog, which) -> {
                    boolean added = store.addCard(front.getText().toString(),
                            back.getText().toString(), example.getText().toString());
                    toast(added ? "Карточка добавлена" : "Нужны слово и перевод");
                    render();
                })
                .show();
    }

    private void confirmReset() {
        new AlertDialog.Builder(this)
                .setTitle(t("Сбросить прогресс?"))
                .setMessage(t("Карточки, XP и статистика этого профиля будут удалены."))
                .setNegativeButton(t("Отмена"), null)
                .setPositiveButton(t("Сбросить"), (dialog, which) -> {
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
        field.setHint(t(hint));
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

    private static final class ExamQuestion {
        final DictionaryEntry entry;
        final boolean ruToEn;
        final List<String> options;
        final int correctIndex;

        ExamQuestion(DictionaryEntry entry, boolean ruToEn, List<String> options, int correctIndex) {
            this.entry = entry;
            this.ruToEn = ruToEn;
            this.options = options;
            this.correctIndex = correctIndex;
        }

        String prompt() {
            return ruToEn ? entry.back : entry.front;
        }
    }

    private String t(String value) {
        if (!store.english() || value == null) return value;
        String result = value;
        if (result.startsWith("Привет, ")) {
            result = "Hello, " + result.substring(8);
        } else if (result.startsWith("Сегодня к повторению — ")) {
            result = "Due today — " + result.substring(23).replace(" карточек.", " cards.");
        } else if (result.startsWith("История: ")) {
            result = "History: " + result.substring(9);
        } else if (result.startsWith("Уровень ")) {
            result = "Level " + result.substring(8);
        } else if (result.startsWith("Серия: ")) {
            result = "Streak: " + result.substring(7);
        } else if (result.startsWith("Упражнение ")) {
            result = "Exercise " + result.substring(11);
        } else if (result.startsWith("Вопрос ")) {
            result = "Question " + result.substring(8);
        } else if (result.startsWith("Всего: ")) {
            result = "Total: " + result.substring(7);
        } else if (result.startsWith("Правильный ответ: ")) {
            result = "Correct answer: " + result.substring(18);
        } else if (result.startsWith("Ошибка. Правильный ответ: ")) {
            result = "Wrong. Correct answer: " + result.substring(26);
        } else if (result.startsWith("LinguaRust Mobile — ")) {
            result = "LinguaRust Mobile is an offline learning app. "
                    + "Dictionary, texts and exercises come from the shared project data folder.";
        }

        switch (result) {
            case "Профиль": return "Profile";
            case "Главная": return "Home";
            case "Карточки": return "Cards";
            case "Словарь": return "Dictionary";
            case "Ещё": return "More";
            case "На сегодня всё повторено. Можно добавить новое слово.":
                return "All caught up for today. Add a new word.";
            case "Цель дня": return "Daily goal";
            case "Дневная цель": return "Daily goal";
            case "Повторять": return "Review";
            case "Открыть карточки": return "Open cards";
            case "дней подряд": return "day streak";
            case "точность": return "accuracy";
            case "слов в работе": return "words in progress";
            case "Слово дня": return "Word of the day";
            case "Добавить в карточки": return "Add to cards";
            case "Слово добавлено": return "Word added";
            case "Слово уже есть в карточках": return "Word is already in your cards";
            case "ПОВТОРЕНИЕ": return "REVIEW";
            case "Опять": return "Again";
            case "Сложно": return "Hard";
            case "Нормально": return "Good";
            case "Легко": return "Easy";
            case "Показать ответ": return "Show answer";
            case "Новое слово": return "New word";
            case "Добавить слово": return "Add word";
            case "Все слова": return "All words";
            case "Все карточки": return "All cards";
            case "Поиск по карточкам": return "Search cards";
            case "Все": return "All";
            case "К повторению": return "Due";
            case "Новые": return "New";
            case "В работе": return "In progress";
            case "Карточки не найдены": return "No cards found";
            case "Карточка": return "Card";
            case "Удалить карточку?": return "Delete card?";
            case "История повторений останется в статистике.":
                return "Review history will remain in statistics.";
            case "Удалить": return "Delete";
            case "Повторить": return "Review";
            case "Карточка удалена": return "Card deleted";
            case "История повторений": return "Review history";
            case "Сравнение карточек": return "Card comparison";
            case "Выбери первую карточку": return "Choose the first card";
            case "Выбери вторую карточку": return "Choose the second card";
            case "Для сравнения добавь хотя бы две карточки.":
                return "Add at least two cards to compare them.";
            case "Карточка 1": return "Card 1";
            case "Карточка 2": return "Card 2";
            case "Выбери разные карточки": return "Choose two different cards";
            case "История пока пуста": return "No history yet";
            case "Успешно · +10 XP": return "Success · +10 XP";
            case "Ошибка · +2 XP": return "Mistake · +2 XP";
            case "Закрыть": return "Close";
            case "340 слов для практики": return "340 words to practise";
            case "Поиск по английскому или русскому": return "Search in English or Russian";
            case "Ничего не найдено": return "Nothing found";
            case "Слово": return "Word";
            case "В карточки": return "To cards";
            case "Практика": return "Practice";
            case "Профиль и локальное хранилище": return "Profile and local storage";
            case "Карточка добавлена": return "Card added";
            case "Нужны слово и перевод": return "Word and translation are required";
            case "Карточки, XP и статистика этого профиля будут удалены.":
                return "Cards, XP and statistics for this profile will be deleted.";
            case "Ошибка. Правильный ответ: ": return "Wrong. Correct answer: ";
            case "Грамматика": return "Grammar";
            case "Чтение": return "Reading";
            case "Статистика": return "Statistics";
            case "Экзамен": return "Exam";
            case "Тренажёр письма": return "Typing trainer";
            case "Перевод → слово с клавиатуры": return "Translation → type the word";
            case "10 вопросов за 90 секунд": return "10 questions in 90 seconds";
            case "Недостаточно слов для экзамена": return "Not enough words for an exam";
            case "Правильных: ": return "Correct: ";
            case "Проверок: ": return "Checks: ";
            case "Перевод": return "Translation";
            case "Введи английское слово": return "Type the English word";
            case "Английское слово": return "English word";
            case "Проверить": return "Check";
            case "Верно! +6 XP": return "Correct! +6 XP";
            case "Ошибка. +1 XP": return "Mistake. +1 XP";
            case "Следующее слово": return "Next word";
            case "Словарь не загрузился": return "Dictionary could not be loaded";
            case "Выбери слово": return "Choose the word";
            case "Выбери перевод": return "Choose the translation";
            case "Верно! +8 XP": return "Correct! +8 XP";
            case "Ошибка. +2 XP": return "Mistake. +2 XP";
            case "Завершить экзамен": return "Finish exam";
            case "Следующий вопрос": return "Next question";
            case "Результат экзамена": return "Exam result";
            case "Экзамен завершён": return "Exam finished";
            case "правильных ответов": return "correct answers";
            case "Пройти ещё раз": return "Try again";
            case "В меню": return "To menu";
            case "Настройки": return "Settings";
            case "27 упражнений с объяснением ошибок": return "27 exercises with explanations";
            case "4 текста уровней A2–B2": return "4 texts from A2 to B2";
            case "XP, стрик, точность и прогресс": return "XP, streak, accuracy and progress";
            case "Упражнения не загрузились": return "Exercises could not be loaded";
            case "Верно! +15 XP": return "Correct! +15 XP";
            case "Следующее упражнение": return "Next exercise";
            case "Выбери текст для практики": return "Choose a text to practise";
            case "Тексты не загрузились": return "Texts could not be loaded";
            case "К списку текстов": return "Back to texts";
            case "Твой прогресс": return "Your progress";
            case "Повторения": return "Reviews";
            case "Серия": return "Streak";
            case "Сбросить локальный прогресс": return "Reset local progress";
            case "Изменить имя": return "Change name";
            case "О приложении": return "About";
            case "Имя": return "Name";
            case "Отмена": return "Cancel";
            case "Сохранить": return "Save";
            case "Слово по-английски": return "English word";
            case "Пример (необязательно)": return "Example (optional)";
            case "Новая карточка": return "New card";
            case "Добавить": return "Add";
            case "Сбросить прогресс?": return "Reset progress?";
            case "Сбросить": return "Reset";
            case "Прогресс сброшен": return "Progress reset";
            case "Русский": return "Russian";
            case "English": return "English";
            case "Язык интерфейса": return "Interface language";
            case "Переключить на русский": return "Switch to Russian";
            case "Переключить на английский": return "Switch to English";
            default:
                break;
        }
        if (result.contains(" повторений в день")) {
            result = result.replace(" повторений в день", " reviews per day");
        }
        if (result.contains("карточек в колоде")) {
            result = result.replace(" карточек в колоде", " cards in the deck");
        }
        if (result.startsWith("Повторения: ")) {
            result = ("Reviews: " + result.substring(13))
                    .replace(" · освоение: ", " · mastery: ");
        } else if (result.startsWith("Интервал: ")) {
            result = "Interval: " + result.substring(10);
        }
        if (result.startsWith("Streak: ")) {
            result = result.replace("следующий интервал: ", "next interval: ")
                    .replace(" дн.", " days");
        }
        if (result.startsWith("Exercise ") || result.startsWith("Question ")) {
            result = result.replace(" из ", " of ");
        }
        if (result.contains(" всего · ")) {
            result = result.replace(" всего · ", " total · ")
                    .replace(" в работе", " in progress");
        }
        if (result.contains(" · точность ")) {
            result = result.replace(" · точность ", " · accuracy ");
        }
        if (result.startsWith("Новое · ")) {
            result = "New · " + result.substring(7);
        } else if (result.startsWith("К повторению · ")) {
            result = "Due · " + result.substring(15);
        } else if (result.startsWith("В работе · ")) {
            result = "In progress · " + result.substring(10);
        }
        if (result.startsWith("New · ") || result.startsWith("Due · ")
                || result.startsWith("In progress · ")) {
            result = result.replace("повторений", "reviews");
        }
        if (result.endsWith(" дн.")) {
            result = result.substring(0, result.length() - 4) + " days";
        }
        if (result.startsWith("Level ")) {
            result = result.replace("Новичок", "Beginner")
                    .replace("Ученик", "Learner")
                    .replace("Практик", "Practitioner")
                    .replace("Знаток", "Expert")
                    .replace("Продвинутый", "Advanced")
                    .replace("Мастер", "Master");
        }
        return result;
    }

    private TextView label(String value, float size, int color) {
        return label(value, size, color, false);
    }

    private TextView label(String value, float size, int color, boolean bold) {
        TextView view = new TextView(this);
        view.setText(t(value));
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
        button.setText(t(title));
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
        Toast.makeText(this, t(message), Toast.LENGTH_SHORT).show();
    }
}
