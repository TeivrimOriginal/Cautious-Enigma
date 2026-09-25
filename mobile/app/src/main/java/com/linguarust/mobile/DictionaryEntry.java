package com.linguarust.mobile;

import java.util.Locale;

/** Слово учебного словаря. */
public final class DictionaryEntry {
    public final String front;
    public final String back;
    public final String example;

    public DictionaryEntry(String front, String back, String example) {
        this.front = front;
        this.back = back;
        this.example = example == null ? "" : example;
    }

    public boolean matches(String query) {
        if (query == null || query.trim().isEmpty()) return true;
        String needle = query.trim().toLowerCase(Locale.ROOT);
        return front.toLowerCase(Locale.ROOT).contains(needle)
                || back.toLowerCase(Locale.ROOT).contains(needle);
    }
}
