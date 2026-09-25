package com.linguarust.mobile;

/** Текст для экрана чтения. */
public final class ReadingText {
    public final String title;
    public final String level;
    public final String summary;
    public final String content;

    public ReadingText(String title, String level, String summary, String content) {
        this.title = title;
        this.level = level;
        this.summary = summary == null ? "" : summary;
        this.content = content;
    }
}
