package com.linguarust.mobile;

import android.content.Context;
import android.content.SharedPreferences;

import org.json.JSONArray;
import org.json.JSONObject;

import java.text.ParseException;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.Date;
import java.util.List;
import java.util.Locale;

/** Локальное хранилище мобильной версии: SharedPreferences + JSON. */
public final class MobileStore {
    private static final String PREFS = "linguarust_mobile";
    private static final String CARDS = "cards";
    private static final String PROFILE = "profile";
    private static final String XP = "xp";
    private static final String REVIEWS = "reviews";
    private static final String CORRECT = "correct";
    private static final String STREAK = "streak";
    private static final String LAST_REVIEW_DAY = "last_review_day";
    private static final String REVIEW_DAY = "review_day";
    private static final String REVIEWS_TODAY = "reviews_today";
    private static final String GRAMMAR_TOTAL = "grammar_total";
    private static final String GRAMMAR_CORRECT = "grammar_correct";
    private static final String REVIEW_LOG = "review_log";
    private static final String ENGLISH = "english";
    private static final String EXAM_TOTAL = "exam_total";
    private static final String EXAM_CORRECT = "exam_correct";

    private final SharedPreferences preferences;
    private final ContentRepository content;
    private final List<CardState> cards = new ArrayList<>();

    public MobileStore(Context context, ContentRepository content) {
        this.content = content;
        preferences = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        loadCards();
        if (cards.isEmpty()) seedStarterCards();
    }

    public ContentRepository content() {
        return content;
    }

    public String profileName() {
        return preferences.getString(PROFILE, "Ученик");
    }

    public void setProfileName(String name) {
        String clean = name == null ? "" : name.trim();
        if (!clean.isEmpty()) {
            preferences.edit().putString(PROFILE, clean).apply();
        }
    }

    public boolean english() {
        return preferences.getBoolean(ENGLISH, false);
    }

    public void setEnglish(boolean enabled) {
        preferences.edit().putBoolean(ENGLISH, enabled).apply();
    }

    public List<CardState> cards() {
        return Collections.unmodifiableList(cards);
    }

    public List<CardState> dueCards() {
        long now = System.currentTimeMillis();
        List<CardState> result = new ArrayList<>();
        for (CardState card : cards) {
            if (card.isDue(now)) result.add(card);
        }
        result.sort(Comparator.comparingLong(value -> value.dueAt));
        return result;
    }

    public int dueCount() {
        return dueCards().size();
    }

    public List<ReviewRecord> historyFor(String front) {
        List<ReviewRecord> result = new ArrayList<>();
        try {
            JSONArray array = new JSONArray(preferences.getString(REVIEW_LOG, "[]"));
            for (int i = array.length() - 1; i >= 0; i--) {
                JSONObject object = array.getJSONObject(i);
                if (!front.equalsIgnoreCase(object.optString("front", ""))) continue;
                result.add(new ReviewRecord(
                        object.optLong("timestamp", 0L),
                        object.optInt("quality", 0)
                ));
                if (result.size() >= 30) break;
            }
        } catch (Exception ignored) {
            return Collections.emptyList();
        }
        return result;
    }

    public int learnedCount() {
        int count = 0;
        for (CardState card : cards) {
            if (card.repetitions > 0) count++;
        }
        return count;
    }

    public int xp() {
        return preferences.getInt(XP, 0);
    }

    public int reviews() {
        return preferences.getInt(REVIEWS, 0);
    }

    public int reviewsToday() {
        rollDayIfNeeded();
        return preferences.getInt(REVIEWS_TODAY, 0);
    }

    public int streak() {
        return preferences.getInt(STREAK, 0);
    }

    public int correctReviews() {
        return preferences.getInt(CORRECT, 0);
    }

    public int examTotal() {
        return preferences.getInt(EXAM_TOTAL, 0);
    }

    public int examCorrect() {
        return preferences.getInt(EXAM_CORRECT, 0);
    }

    public void recordExam(boolean correct) {
        preferences.edit()
                .putInt(EXAM_TOTAL, examTotal() + 1)
                .putInt(EXAM_CORRECT, examCorrect() + (correct ? 1 : 0))
                .putInt(XP, xp() + (correct ? 8 : 2))
                .apply();
    }

    public int grammarTotal() {
        return preferences.getInt(GRAMMAR_TOTAL, 0);
    }

    public int grammarCorrect() {
        return preferences.getInt(GRAMMAR_CORRECT, 0);
    }

    public int accuracy() {
        int total = reviews();
        if (total == 0) return 0;
        return Math.round(correctReviews() * 100f / total);
    }

    public LevelInfo level() {
        int totalXp = xp();
        int level = 1;
        int remaining = totalXp;
        int threshold = 100;
        while (remaining >= threshold) {
            remaining -= threshold;
            level++;
            threshold = 50;
        }
        int needed = level == 1 ? 100 : 50;
        return new LevelInfo(level, remaining, needed, levelTitle(level));
    }

    public boolean addCard(String front, String back, String example) {
        String cleanFront = clean(front);
        String cleanBack = clean(back);
        if (cleanFront.isEmpty() || cleanBack.isEmpty()) return false;
        for (CardState card : cards) {
            if (card.front.equalsIgnoreCase(cleanFront)) return false;
        }
        CardState card = new CardState(cleanFront, cleanBack, clean(example));
        card.dueAt = System.currentTimeMillis();
        cards.add(card);
        preferences.edit().putInt(XP, xp() + 5).apply();
        saveCards();
        return true;
    }

    /** Возвращает true, если карточка была найдена и обновлена. */
    public boolean review(CardState target, int quality) {
        CardState card = find(target.front);
        if (card == null) return false;
        long now = System.currentTimeMillis();
        Sm2.apply(card, quality, now);

        boolean successful = quality >= 3;
        SharedPreferences.Editor editor = preferences.edit()
                .putInt(REVIEWS, reviews() + 1)
                .putInt(CORRECT, correctReviews() + (successful ? 1 : 0))
                .putInt(XP, xp() + (successful ? 10 : 2));
        updateStreak(editor, now);
        editor.apply();
        appendReview(card.front, quality, now);
        saveCards();
        return true;
    }

    public boolean recordGrammar(boolean correct) {
        SharedPreferences.Editor editor = preferences.edit()
                .putInt(GRAMMAR_TOTAL, grammarTotal() + 1)
                .putInt(GRAMMAR_CORRECT, grammarCorrect() + (correct ? 1 : 0))
                .putInt(XP, xp() + (correct ? 15 : 3));
        editor.apply();
        return correct;
    }

    public void resetProgress() {
        preferences.edit()
                .remove(CARDS)
                .remove(XP)
                .remove(REVIEWS)
                .remove(CORRECT)
                .remove(STREAK)
                .remove(LAST_REVIEW_DAY)
                .remove(REVIEW_DAY)
                .remove(REVIEWS_TODAY)
                .remove(GRAMMAR_TOTAL)
                .remove(GRAMMAR_CORRECT)
                .remove(REVIEW_LOG)
                .remove(EXAM_TOTAL)
                .remove(EXAM_CORRECT)
                .apply();
        cards.clear();
        seedStarterCards();
    }

    private void loadCards() {
        String raw = preferences.getString(CARDS, "[]");
        try {
            JSONArray array = new JSONArray(raw);
            for (int i = 0; i < array.length(); i++) {
                JSONObject object = array.getJSONObject(i);
                String front = object.optString("front", "").trim();
                String back = object.optString("back", "").trim();
                if (!front.isEmpty() && !back.isEmpty()) {
                    cards.add(CardState.fromJson(object));
                }
            }
        } catch (Exception ignored) {
            cards.clear();
        }
    }

    private void saveCards() {
        JSONArray array = new JSONArray();
        try {
            for (CardState card : cards) array.put(card.toJson());
        } catch (Exception ignored) {
            return;
        }
        preferences.edit().putString(CARDS, array.toString()).apply();
    }

    private void appendReview(String front, int quality, long timestamp) {
        try {
            JSONArray array = new JSONArray(preferences.getString(REVIEW_LOG, "[]"));
            JSONObject object = new JSONObject();
            object.put("front", front);
            object.put("quality", quality);
            object.put("timestamp", timestamp);
            array.put(object);

            // Не разрастаемся бесконечно: для анализа достаточно 200 записей.
            JSONArray trimmed = new JSONArray();
            int start = Math.max(0, array.length() - 200);
            for (int i = start; i < array.length(); i++) {
                trimmed.put(array.get(i));
            }
            preferences.edit().putString(REVIEW_LOG, trimmed.toString()).apply();
        } catch (Exception ignored) {
            // Журнал не должен блокировать повторение карточки.
        }
    }

    private void seedStarterCards() {
        int limit = Math.min(12, content.dictionary().size());
        for (int i = 0; i < limit; i++) {
            DictionaryEntry entry = content.dictionary().get(i);
            cards.add(new CardState(entry.front, entry.back, entry.example));
        }
        saveCards();
    }

    private CardState find(String front) {
        for (CardState card : cards) {
            if (card.front.equalsIgnoreCase(front)) return card;
        }
        return null;
    }

    private void rollDayIfNeeded() {
        String today = dateKey(System.currentTimeMillis());
        if (!today.equals(preferences.getString(REVIEW_DAY, ""))) {
            preferences.edit()
                    .putString(REVIEW_DAY, today)
                    .putInt(REVIEWS_TODAY, 0)
                    .apply();
        }
    }

    private void updateStreak(SharedPreferences.Editor editor, long now) {
        String today = dateKey(now);
        if (today.equals(preferences.getString(REVIEW_DAY, ""))) {
            preferences.edit().putInt(REVIEWS_TODAY, reviewsToday() + 1).apply();
            return;
        }

        String last = preferences.getString(LAST_REVIEW_DAY, "");
        int nextStreak = last.isEmpty() || daysBetween(last, today) != 1
                ? 1
                : preferences.getInt(STREAK, 0) + 1;
        editor.putString(REVIEW_DAY, today)
                .putInt(REVIEWS_TODAY, 1)
                .putString(LAST_REVIEW_DAY, today)
                .putInt(STREAK, nextStreak);
    }

    private static String clean(String value) {
        return value == null ? "" : value.trim();
    }

    private static String dateKey(long timestamp) {
        return new SimpleDateFormat("yyyy-MM-dd", Locale.US).format(new Date(timestamp));
    }

    private static int daysBetween(String from, String to) {
        SimpleDateFormat format = new SimpleDateFormat("yyyy-MM-dd", Locale.US);
        format.setLenient(false);
        try {
            Date first = format.parse(from);
            Date second = format.parse(to);
            long difference = second.getTime() - first.getTime();
            return (int) Math.round(difference / (double) Sm2.DAY_MS);
        } catch (ParseException error) {
            return Integer.MAX_VALUE;
        }
    }

    private static String levelTitle(int level) {
        switch (level) {
            case 1:
                return "Новичок";
            case 2:
                return "Ученик";
            case 3:
                return "Практик";
            case 4:
                return "Знаток";
            case 5:
                return "Продвинутый";
            default:
                return "Мастер";
        }
    }

    public static final class LevelInfo {
        public final int level;
        public final int inLevel;
        public final int needed;
        public final int percent;
        public final String title;

        private LevelInfo(int level, int inLevel, int needed, String title) {
            this.level = level;
            this.inLevel = inLevel;
            this.needed = needed;
            this.percent = Math.min(100, Math.round(inLevel * 100f / needed));
            this.title = title;
        }
    }
}
